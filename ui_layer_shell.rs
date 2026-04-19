use std::collections::HashSet;
use std::fs;
use std::num::NonZeroU32;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use font8x8::UnicodeFonts;
use fontdue::{Font, FontSettings};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_touch,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        touch::TouchHandler,
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface, wl_touch},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_misc::zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{self, ZwpInputMethodV2},
};

use crate::layout::{self, KeyAction, Layer as LayoutLayer, Mod};
use crate::shell_state::{ShellState, ShellStateMachine};
use crate::vk::VkCommand;

const FULL_W: u32 = 1100;
const FULL_H: u32 = 340;
const COLLAPSED_H: u32 = 56;
const TOUCH_LAUNCHER_H: u32 = 24;
const HIDDEN_W: u32 = 1;
const HIDDEN_H: u32 = 1;

const PADDING: f32 = 8.0;
const GAP: f32 = 4.0;
const KEY_H: f32 = 52.0;
const HEADER_H: f32 = 36.0;
const KEY_BORDER: f32 = 1.0;

fn load_preferred_font() -> Option<Font> {
    let candidates = [
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-B.ttf",
        "/usr/share/fonts/truetype/ubuntu/UbuntuSans[wdth,wght].ttf",
        "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) {
                tracing::info!("using font for key labels: {path}");
                return Some(font);
            }
        }
    }
    tracing::warn!("no system mono font found; falling back to bitmap labels");
    None
}

fn load_symbol_fallback_font() -> Option<Font> {
    let candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansCondensed.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(font) = Font::from_bytes(bytes, FontSettings::default()) {
                tracing::info!("using symbol fallback font: {path}");
                return Some(font);
            }
        }
    }
    None
}

#[derive(Debug, Clone)]
enum HitTarget {
    Expand,
    Dismiss,
    Key(KeyAction, String),
}

#[derive(Debug, Clone)]
struct HitBox {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    target: HitTarget,
}

#[derive(Debug, Clone)]
struct FlashKey {
    action: KeyAction,
    label: String,
    until: Instant,
}

pub fn run(vk_tx: mpsc::Sender<VkCommand>, touch_first_only: bool) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let shm = Shm::bind(&globals, &qh)?;

    let surface = compositor_state.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        Layer::Top,
        Some("cosmic-touch-keyboard"),
        None,
    );
    layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_size(FULL_W, FULL_H);
    layer.commit();

    let pool = SlotPool::new((FULL_W * FULL_H * 4) as usize, &shm)?;

    let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=9, ())?;
    let (im_manager, input_method) =
        match globals.bind::<ZwpInputMethodManagerV2, LayerKeyboard, ()>(&qh, 1..=1, ()) {
            Ok(mgr) => {
                let im = mgr.get_input_method(&seat, &qh, ());
                tracing::info!("input method v2 bound in layer-shell frontend");
                (Some(mgr), Some(im))
            }
            Err(e) => {
                tracing::warn!(
                    "input_method_v2 unavailable ({e}); keyboard will stay available all the time"
                );
                (None, None)
            }
        };

    let mut app = LayerKeyboard {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        _compositor_state: compositor_state,
        shm,

        exit: false,
        first_configure: true,
        width: FULL_W,
        height: FULL_H,
        pool,

        layer,
        pointer: None,
        touch: None,

        _seat: seat,
        _im_manager: im_manager,
        _input_method: input_method,
        im_available: false,

        state_machine: ShellStateMachine::new(),
        touch_first_only,
        hitboxes: Vec::new(),

        vk_tx,
        layer_sel: LayoutLayer::Letters,
        armed: HashSet::new(),
        locked: HashSet::new(),
        needs_redraw: false,
        font: load_preferred_font(),
        symbol_font: load_symbol_fallback_font(),
        flashes: Vec::new(),
    };

    if app._input_method.is_none() {
        app.state_machine.on_im_activate(false);
        app.im_available = false;
    } else {
        app.im_available = true;
    }
    app.apply_shell();

    loop {
        event_queue.blocking_dispatch(&mut app)?;
        if app.needs_redraw {
            app.draw(&qh)?;
            app.needs_redraw = false;
        }
        if app.exit {
            break;
        }
    }

    Ok(())
}

struct LayerKeyboard {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    _compositor_state: CompositorState,
    shm: Shm,

    exit: bool,
    first_configure: bool,
    width: u32,
    height: u32,
    pool: SlotPool,

    layer: LayerSurface,
    pointer: Option<wl_pointer::WlPointer>,
    touch: Option<wl_touch::WlTouch>,

    _seat: wl_seat::WlSeat,
    _im_manager: Option<ZwpInputMethodManagerV2>,
    _input_method: Option<ZwpInputMethodV2>,
    im_available: bool,

    state_machine: ShellStateMachine,
    touch_first_only: bool,
    hitboxes: Vec<HitBox>,

    vk_tx: mpsc::Sender<VkCommand>,
    layer_sel: LayoutLayer,
    armed: HashSet<Mod>,
    locked: HashSet<Mod>,
    needs_redraw: bool,
    font: Option<Font>,
    symbol_font: Option<Font>,
    flashes: Vec<FlashKey>,
}

impl LayerKeyboard {
    fn apply_shell(&mut self) {
        match self.state_machine.state {
            ShellState::Hidden => {
                if self.touch_first_only && self.state_machine.should_show_touch_launcher() {
                    self.layer.set_size(FULL_W, TOUCH_LAUNCHER_H);
                } else {
                    self.layer.set_size(HIDDEN_W, HIDDEN_H);
                }
                self.layer.set_exclusive_zone(0);
                self.layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            }
            ShellState::CollapsedReady => {
                self.layer.set_size(FULL_W, COLLAPSED_H);
                self.layer.set_exclusive_zone(COLLAPSED_H as i32);
                self.layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            }
            ShellState::Expanded => {
                self.layer.set_size(FULL_W, FULL_H);
                self.layer.set_exclusive_zone(FULL_H as i32);
                self.layer.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
            }
        }
        self.layer.commit();
        self.needs_redraw = true;
    }

    fn on_im_activate(&mut self) {
        tracing::info!("input-method activate (text field focused)");
        self.state_machine.on_im_activate(self.touch_first_only);
        self.apply_shell();
    }

    fn on_im_deactivate(&mut self) {
        tracing::info!("input-method deactivate (focus left text field)");
        self.state_machine.on_im_deactivate();
        self.apply_shell();
    }

    fn on_point_down(&mut self, x: f32, y: f32) {
        let Some(target) = self
            .hitboxes
            .iter()
            .rev()
            .find(|h| x >= h.x && y >= h.y && x <= h.x + h.w && y <= h.y + h.h)
            .map(|h| h.target.clone())
        else {
            return;
        };

        match target {
            HitTarget::Expand => {
                self.state_machine.on_touch_expand();
                self.apply_shell();
            }
            HitTarget::Dismiss => {
                self.state_machine.on_user_dismiss();
                self.apply_shell();
            }
            HitTarget::Key(action, label) => self.on_key(action, label),
        }
    }

    fn push_flash(&mut self, action: &KeyAction, label: &str) {
        self.flashes.push(FlashKey {
            action: action.clone(),
            label: label.to_string(),
            until: Instant::now() + Duration::from_millis(140),
        });
        self.needs_redraw = true;
    }

    fn on_key(&mut self, action: KeyAction, label: String) {
        self.push_flash(&action, &label);
        match action {
            KeyAction::Modifier(m) => self.toggle_mod(m),
            KeyAction::Layer(l) => {
                self.layer_sel = l;
                self.needs_redraw = true;
            }
            KeyAction::Hide => {
                self.state_machine.on_user_dismiss();
                self.apply_shell();
            }
            KeyAction::Code(code) => {
                if code == 0 {
                    self.inject_text(&label);
                } else {
                    self.send_mods_and_key(code);
                }
                self.clear_armed_mods();
            }
        }
    }

    fn toggle_mod(&mut self, m: Mod) {
        if self.locked.contains(&m) {
            self.locked.remove(&m);
        } else if self.armed.contains(&m) {
            self.armed.remove(&m);
            self.locked.insert(m);
        } else {
            self.armed.insert(m);
        }
        self.push_mod_state();
        self.needs_redraw = true;
    }

    fn mod_active(&self, m: Mod) -> bool {
        self.armed.contains(&m) || self.locked.contains(&m)
    }

    fn current_mod_mask(&self) -> u32 {
        let mut mask = 0u32;
        for m in [Mod::Shift, Mod::Ctrl, Mod::Alt, Mod::Super] {
            if self.mod_active(m) {
                mask |= m.mask_bit();
            }
        }
        mask
    }

    fn push_mod_state(&self) {
        let _ = self.vk_tx.send(VkCommand::Modifiers(self.current_mod_mask()));
    }

    fn send_mods_and_key(&self, code: u32) {
        let _ = self.vk_tx.send(VkCommand::Modifiers(self.current_mod_mask()));
        let _ = self.vk_tx.send(VkCommand::Tap(code));
    }

    fn clear_armed_mods(&mut self) {
        if !self.armed.is_empty() {
            self.armed.clear();
            self.push_mod_state();
            self.needs_redraw = true;
        }
    }

    fn inject_text(&self, s: &str) {
        if Command::new("wtype").arg("--").arg(s).status().is_ok() {
            return;
        }
        if let Ok(mut child) = Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            use std::io::Write;
            if let Some(stdin) = child.stdin.as_mut() {
                let _ = stdin.write_all(s.as_bytes());
            }
            let _ = child.wait();

            let _ = self.vk_tx.send(VkCommand::Modifiers(Mod::Ctrl.mask_bit()));
            let _ = self.vk_tx.send(VkCommand::Tap(layout::kc::V));
            let _ = self.vk_tx.send(VkCommand::Modifiers(0));
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) -> anyhow::Result<()> {
        let now = Instant::now();
        self.flashes.retain(|f| f.until > now);

        let flash_snapshot: Vec<(KeyAction, String)> = self
            .flashes
            .iter()
            .map(|f| (f.action.clone(), f.label.clone()))
            .collect();

        let width = self.width.max(1);
        let height = self.height.max(1);
        let stride = width as i32 * 4;
        let shift_on = self.mod_active(Mod::Shift);
        let active_layer = self.layer_sel;
        let rows_for_draw = if self.state_machine.state == ShellState::Expanded {
            Some(layout::layout_for(active_layer))
        } else {
            None
        };

        let (buffer, canvas) = self
            .pool
            .create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888)?;

        self.hitboxes.clear();

        fill_rect(canvas, width, 0.0, 0.0, width as f32, height as f32, 0xCC111111);

        match self.state_machine.state {
            ShellState::Hidden => {
                if self.touch_first_only && self.state_machine.should_show_touch_launcher() {
                    fill_rect(canvas, width, 0.0, 0.0, width as f32, height as f32, 0xAA141414);
                    stroke_rect(
                        canvas,
                        width,
                        0.0,
                        0.0,
                        width as f32,
                        height as f32,
                        0xFF3D3D3D,
                        1.0,
                    );
                    draw_label_center(
                        canvas,
                        width,
                        0.0,
                        0.0,
                        width as f32,
                        height as f32,
                        "Tap here to open keyboard",
                        0xFFDCDCDC,
                        self.font.as_ref(),
                        self.symbol_font.as_ref(),
                    );
                    self.hitboxes.push(HitBox {
                        x: 0.0,
                        y: 0.0,
                        w: width as f32,
                        h: height as f32,
                        target: HitTarget::Expand,
                    });
                }
            }
            ShellState::CollapsedReady => {
                let dismiss_w = 44.0;
                fill_rect(
                    canvas,
                    width,
                    width as f32 - dismiss_w - 8.0,
                    6.0,
                    dismiss_w,
                    44.0,
                    0xFF9A2A2A,
                );
                draw_label_center(
                    canvas,
                    width,
                    width as f32 - dismiss_w - 8.0,
                    6.0,
                    dismiss_w,
                    44.0,
                    "X",
                    0xFFFFFFFF,
                    self.font.as_ref(),
                    self.symbol_font.as_ref(),
                );
                self.hitboxes.push(HitBox {
                    x: 0.0,
                    y: 0.0,
                    w: width as f32 - dismiss_w - 8.0,
                    h: height as f32,
                    target: HitTarget::Expand,
                });
                self.hitboxes.push(HitBox {
                    x: width as f32 - dismiss_w - 8.0,
                    y: 6.0,
                    w: dismiss_w,
                    h: 44.0,
                    target: HitTarget::Dismiss,
                });
            }
            ShellState::Expanded => {
                let dismiss_w = 36.0;
                fill_rect(
                    canvas,
                    width,
                    width as f32 - dismiss_w - 8.0,
                    0.0,
                    dismiss_w,
                    32.0,
                    0xFF9A2A2A,
                );
                draw_label_center(
                    canvas,
                    width,
                    width as f32 - dismiss_w - 8.0,
                    0.0,
                    dismiss_w,
                    32.0,
                    "X",
                    0xFFFFFFFF,
                    self.font.as_ref(),
                    self.symbol_font.as_ref(),
                );
                self.hitboxes.push(HitBox {
                    x: width as f32 - dismiss_w - 8.0,
                    y: 0.0,
                    w: dismiss_w,
                    h: 32.0,
                    target: HitTarget::Dismiss,
                });

                let rows = rows_for_draw.unwrap_or_default();
                let max_units = rows
                    .iter()
                    .map(|r| r.iter().map(|k| k.width).sum::<f32>())
                    .fold(10.0, f32::max);
                let max_count = rows.iter().map(std::vec::Vec::len).max().unwrap_or(10);
                let avail_w = width as f32 - (PADDING * 2.0) - GAP * (max_count.saturating_sub(1) as f32);
                let unit_w = (avail_w / max_units).max(20.0);

                let mut y = HEADER_H;
                for row in rows {
                    let row_units: f32 = row.iter().map(|k| k.width).sum();
                    let row_w = row_units * unit_w + GAP * (row.len().saturating_sub(1) as f32);
                    let mut x = ((width as f32 - row_w) * 0.5).max(PADDING);
                    for key in row {
                        let key_w = key.width * unit_w;
                        let mut display_label = if shift_on {
                            key.shifted
                                .clone()
                                .unwrap_or_else(|| key.label.clone())
                        } else {
                            key.label.clone()
                        };
                        display_label = normalize_key_label(&display_label);
                        let active = match &key.action {
                            KeyAction::Modifier(m) => self.armed.contains(m) || self.locked.contains(m),
                            KeyAction::Layer(l) => *l == active_layer,
                            _ => false,
                        };
                        let flashing = is_flashing_in(&flash_snapshot, &key.action, &key.label);
                        let (fill, border) = key_palette(&key.action, active, flashing);
                        fill_rect(canvas, width, x + 1.0, y + 1.0, key_w, KEY_H, 0x66000000);
                        fill_rect(canvas, width, x, y, key_w, KEY_H, fill);
                        stroke_rect(
                            canvas,
                            width,
                            x,
                            y,
                            key_w,
                            KEY_H,
                            border,
                            KEY_BORDER,
                        );
                        draw_label_center(
                            canvas,
                            width,
                            x,
                            y,
                            key_w,
                            KEY_H,
                            &display_label,
                            0xFFFFFFFF,
                            self.font.as_ref(),
                            self.symbol_font.as_ref(),
                        );
                        self.hitboxes.push(HitBox {
                            x,
                            y,
                            w: key_w,
                            h: KEY_H,
                            target: HitTarget::Key(key.action.clone(), key.label.clone()),
                        });
                        x += key_w + GAP;
                    }
                    y += KEY_H + GAP;
                }
            }
        }

        self.layer.wl_surface().damage_buffer(0, 0, width as i32, height as i32);
        self.layer
            .wl_surface()
            .frame(qh, self.layer.wl_surface().clone());
        buffer.attach_to(self.layer.wl_surface())?;
        self.layer.commit();
        Ok(())
    }
}

fn fill_rect(
    canvas: &mut [u8],
    canvas_w: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: u32,
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let x0 = x.max(0.0) as u32;
    let y0 = y.max(0.0) as u32;
    let x1 = (x + w).max(0.0) as u32;
    let y1 = (y + h).max(0.0) as u32;
    for yy in y0..y1 {
        for xx in x0..x1 {
            let idx = ((yy * canvas_w + xx) * 4) as usize;
            if idx + 4 <= canvas.len() {
                let px = color.to_le_bytes();
                canvas[idx] = px[0];
                canvas[idx + 1] = px[1];
                canvas[idx + 2] = px[2];
                canvas[idx + 3] = px[3];
            }
        }
    }
}

fn stroke_rect(
    canvas: &mut [u8],
    canvas_w: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: u32,
    thickness: f32,
) {
    if thickness <= 0.0 || w <= 0.0 || h <= 0.0 {
        return;
    }
    fill_rect(canvas, canvas_w, x, y, w, thickness, color);
    fill_rect(canvas, canvas_w, x, y + h - thickness, w, thickness, color);
    fill_rect(canvas, canvas_w, x, y, thickness, h, color);
    fill_rect(canvas, canvas_w, x + w - thickness, y, thickness, h, color);
}

fn key_palette(action: &KeyAction, active: bool, flashing: bool) -> (u32, u32) {
    if flashing {
        // brief tap feedback
        return (0xFFD89A4C, 0xFFF1C88F);
    }
    match action {
        KeyAction::Hide => (0xFF8C2020, 0xFFB64545),
        KeyAction::Layer(_) if active => (0xFFCC8A2D, 0xFFE9B66C),
        KeyAction::Layer(_) => (0xFF2B2B2B, 0xFF4A4A4A),
        KeyAction::Modifier(_) if active => (0xFFCC8A2D, 0xFFE9B66C),
        KeyAction::Modifier(_) => (0xFF303030, 0xFF525252),
        KeyAction::Code(_) => (0xFF242424, 0xFF4A4A4A),
    }
}

fn key_action_eq(a: &KeyAction, b: &KeyAction) -> bool {
    match (a, b) {
        (KeyAction::Code(x), KeyAction::Code(y)) => x == y,
        (KeyAction::Layer(x), KeyAction::Layer(y)) => x == y,
        (KeyAction::Modifier(x), KeyAction::Modifier(y)) => x == y,
        (KeyAction::Hide, KeyAction::Hide) => true,
        _ => false,
    }
}

fn is_flashing_in(snapshot: &[(KeyAction, String)], action: &KeyAction, label: &str) -> bool {
    snapshot
        .iter()
        .any(|(a, l)| key_action_eq(a, action) && l == label)
}

fn normalize_key_label(label: &str) -> String {
    match label {
        "⌫" => "Backspace".to_string(),
        "⇧" => "Shift".to_string(),
        "⎋" => "Esc".to_string(),
        "⌨" => "Hide".to_string(),
        "ABC" => "Letters".to_string(),
        "?123" => "Symbols".to_string(),
        "😊" => "Emoji".to_string(),
        "…" => "...".to_string(),
        other => other.to_string(),
    }
}

fn draw_label_center(
    canvas: &mut [u8],
    canvas_w: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: &str,
    color: u32,
    font: Option<&Font>,
    symbol_font: Option<&Font>,
) {
    let text = label.trim();
    if text.is_empty() {
        return;
    }

    if let Some(font) = font {
        draw_label_fontdue(canvas, canvas_w, x, y, w, h, text, color, font, symbol_font);
        return;
    }

    let scale = if w > 110.0 { 2 } else { 1 };
    let glyph_w = 8 * scale;
    let glyph_h = 8 * scale;
    let spacing = scale;
    let max_chars = ((w.max(1.0) as i32) / (glyph_w + spacing)).max(1) as usize;
    let shown: String = text.chars().take(max_chars).collect();
    let text_w = shown.chars().count() as i32 * (glyph_w + spacing) - spacing;
    let start_x = x as i32 + ((w as i32 - text_w).max(0) / 2);
    let start_y = y as i32 + ((h as i32 - glyph_h).max(0) / 2);

    for (i, ch) in shown.chars().enumerate() {
        let gx = start_x + i as i32 * (glyph_w + spacing);
        draw_glyph(canvas, canvas_w, gx, start_y, ch, scale as usize, color);
    }
}

fn draw_label_fontdue(
    canvas: &mut [u8],
    canvas_w: u32,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    text: &str,
    color: u32,
    font: &Font,
    symbol_font: Option<&Font>,
) {
    let char_count = text.chars().count().max(1) as f32;
    let width_budget = (w / char_count).clamp(8.0, 30.0);
    let px = (h * 0.50).min(width_budget * 1.35).clamp(13.5, 24.0);
    let mut advances = Vec::new();
    let mut pen = 0.0f32;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for ch in text.chars() {
        let picked = pick_font_for_char(font, symbol_font, ch);
        let metrics = picked.metrics(ch, px);
        if metrics.width > 0 {
            let gx0 = pen + metrics.xmin as f32;
            let gx1 = gx0 + metrics.width as f32;
            min_x = min_x.min(gx0);
            max_x = max_x.max(gx1);
        }
        if metrics.height > 0 {
            // Font metrics are baseline-relative in Y-up space. Convert into Y-down space.
            // This preserves descenders (y, p, g, etc.) instead of flattening them.
            let gy0 = -(metrics.ymin as f32 + metrics.height as f32);
            let gy1 = gy0 + metrics.height as f32;
            min_y = min_y.min(gy0);
            max_y = max_y.max(gy1);
        }
        advances.push((ch, metrics, picked, pen));
        pen += metrics.advance_width.max(1.0);
    }
    if advances.is_empty() {
        return;
    }

    if !min_x.is_finite() || !max_x.is_finite() {
        min_x = 0.0;
        max_x = pen.max(1.0);
    }
    if !min_y.is_finite() || !max_y.is_finite() {
        min_y = 0.0;
        max_y = px * 0.75;
    }

    let text_w = (max_x - min_x).max(1.0);
    let text_h = (max_y - min_y).max(1.0);
    let origin_x = x + ((w - text_w).max(0.0) * 0.5) - min_x;
    let origin_y = y + ((h - text_h).max(0.0) * 0.5) - min_y;

    for (ch, _metrics, picked_font, pen_x) in advances {
        let (metrics, bitmap) = picked_font.rasterize(ch, px);
        let gy0 = -(metrics.ymin as f32 + metrics.height as f32);
        for by in 0..metrics.height {
            for bx in 0..metrics.width {
                let alpha = bitmap[by * metrics.width + bx];
                if alpha == 0 {
                    continue;
                }
                let px_x = origin_x + pen_x + bx as f32 + metrics.xmin as f32;
                let px_y = origin_y + gy0 + by as f32;
                put_pixel_alpha(canvas, canvas_w, px_x as i32, px_y as i32, color, alpha);
            }
        }
    }
}

fn pick_font_for_char<'a>(primary: &'a Font, fallback: Option<&'a Font>, ch: char) -> &'a Font {
    if primary.lookup_glyph_index(ch) != 0 {
        return primary;
    }
    if let Some(fb) = fallback {
        if fb.lookup_glyph_index(ch) != 0 {
            return fb;
        }
    }
    primary
}

fn draw_glyph(
    canvas: &mut [u8],
    canvas_w: u32,
    x: i32,
    y: i32,
    ch: char,
    scale: usize,
    color: u32,
) {
    let Some(bitmap) = font8x8::BASIC_FONTS.get(ch) else {
        return;
    };

    for (row, bits) in bitmap.iter().enumerate() {
        for col in 0..8 {
            if ((bits >> col) & 1) == 0 {
                continue;
            }
            let px = x + col * scale as i32;
            let py = y + row as i32 * scale as i32;
            for sy in 0..scale as i32 {
                for sx in 0..scale as i32 {
                    put_pixel(canvas, canvas_w, px + sx, py + sy, color);
                }
            }
        }
    }
}

fn put_pixel(canvas: &mut [u8], canvas_w: u32, x: i32, y: i32, color: u32) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as u32;
    let y = y as u32;
    let idx = ((y * canvas_w + x) * 4) as usize;
    if idx + 4 <= canvas.len() {
        let px = color.to_le_bytes();
        canvas[idx] = px[0];
        canvas[idx + 1] = px[1];
        canvas[idx + 2] = px[2];
        canvas[idx + 3] = px[3];
    }
}

fn put_pixel_alpha(
    canvas: &mut [u8],
    canvas_w: u32,
    x: i32,
    y: i32,
    color: u32,
    alpha: u8,
) {
    if x < 0 || y < 0 {
        return;
    }
    let x = x as u32;
    let y = y as u32;
    let idx = ((y * canvas_w + x) * 4) as usize;
    if idx + 4 > canvas.len() {
        return;
    }

    let src = color.to_le_bytes();
    let a = alpha as u16;
    for i in 0..3 {
        let dst = canvas[idx + i] as u16;
        let srcv = src[i] as u16;
        canvas[idx + i] = (((srcv * a) + (dst * (255 - a))) / 255) as u8;
    }
    canvas[idx + 3] = 0xFF;
}

impl CompositorHandler for LayerKeyboard {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        let _ = self.draw(qh);
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for LayerKeyboard {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for LayerKeyboard {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width = NonZeroU32::new(configure.new_size.0).map_or(self.width.max(1), NonZeroU32::get);
        self.height = NonZeroU32::new(configure.new_size.1).map_or(self.height.max(1), NonZeroU32::get);
        if self.first_configure {
            self.first_configure = false;
        }
        let _ = self.draw(qh);
    }
}

impl SeatHandler for LayerKeyboard {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            if let Ok(pointer) = self.seat_state.get_pointer(qh, &seat) {
                self.pointer = Some(pointer);
            }
        }
        if capability == Capability::Touch && self.touch.is_none() {
            if let Ok(touch) = self.seat_state.get_touch(qh, &seat) {
                self.touch = Some(touch);
            }
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            if let Some(pointer) = self.pointer.take() {
                pointer.release();
            }
        }
        if capability == Capability::Touch {
            if let Some(touch) = self.touch.take() {
                touch.release();
            }
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for LayerKeyboard {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.layer.wl_surface() {
                continue;
            }
            if let PointerEventKind::Press { .. } = event.kind {
                if self.state_machine.state == ShellState::CollapsedReady {
                    // Touch-first behavior: do not expand from mouse/pointer clicks.
                    continue;
                }
                self.on_point_down(event.position.0 as f32, event.position.1 as f32);
            }
        }
    }
}

impl TouchHandler for LayerKeyboard {
    fn down(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _serial: u32,
        _time: u32,
        surface: wl_surface::WlSurface,
        _id: i32,
        position: (f64, f64),
    ) {
        if surface == *self.layer.wl_surface() {
            self.on_point_down(position.0 as f32, position.1 as f32);
        }
    }

    fn up(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _serial: u32,
        _time: u32,
        _id: i32,
    ) {
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _time: u32,
        _id: i32,
        _position: (f64, f64),
    ) {
    }

    fn shape(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _id: i32,
        _major: f64,
        _minor: f64,
    ) {
    }

    fn orientation(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _touch: &wl_touch::WlTouch,
        _id: i32,
        _orientation: f64,
    ) {
    }

    fn cancel(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _touch: &wl_touch::WlTouch) {}
}

impl ShmHandler for LayerKeyboard {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for LayerKeyboard {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodManagerV2, ()> for LayerKeyboard {
    fn event(
        _: &mut Self,
        _: &ZwpInputMethodManagerV2,
        _: <ZwpInputMethodManagerV2 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpInputMethodV2, ()> for LayerKeyboard {
    fn event(
        state: &mut Self,
        _: &ZwpInputMethodV2,
        event: <ZwpInputMethodV2 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwp_input_method_v2::Event;
        match event {
            Event::Activate => state.on_im_activate(),
            Event::Deactivate => state.on_im_deactivate(),
            _ => {}
        }
    }
}

delegate_compositor!(LayerKeyboard);
delegate_output!(LayerKeyboard);
delegate_shm!(LayerKeyboard);
delegate_seat!(LayerKeyboard);
delegate_pointer!(LayerKeyboard);
delegate_touch!(LayerKeyboard);
delegate_layer!(LayerKeyboard);
delegate_registry!(LayerKeyboard);

impl ProvidesRegistryState for LayerKeyboard {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}
