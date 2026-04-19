//! Wayland virtual keyboard backend.
//!
//! This thread owns `zwp_virtual_keyboard_v1` and receives key/modifier commands
//! from the UI process.

use std::io::Write;
use std::os::fd::AsFd;
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use wayland_client::{
    globals::{registry_queue_init, GlobalListContents},
    protocol::{wl_registry, wl_seat::WlSeat},
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

/// Commands the UI sends into the Wayland thread.
#[derive(Debug, Clone, Copy)]
pub enum VkCommand {
    /// Press then release a keycode (XKB keycode, i.e. evdev + 8).
    Tap(u32),
    /// Set the currently-held modifier mask (Shift=1, Ctrl=4, Alt=8, Super=64 — xkb bitfield).
    Modifiers(u32),
    #[allow(dead_code)]
    Press(u32),
    #[allow(dead_code)]
    Release(u32),
}

pub struct WaylandThread {
    pub vk_tx: mpsc::Sender<VkCommand>,
}

pub fn spawn_wayland_thread() -> WaylandThread {
    let (tx, rx) = mpsc::channel::<VkCommand>();

    thread::Builder::new()
        .name("cosmic-kb-wayland".into())
        .spawn(move || {
            if let Err(e) = run(rx) {
                tracing::error!("virtual keyboard thread exited: {e:#}");
            }
        })
        .expect("spawn wayland thread");

    WaylandThread { vk_tx: tx }
}

fn run(rx: mpsc::Receiver<VkCommand>) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();

    let seat: WlSeat = globals.bind(&qh, 1..=9, ())?;
    let vk_manager: ZwpVirtualKeyboardManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .map_err(|e| anyhow::anyhow!(
            "compositor does not expose zwp_virtual_keyboard_manager_v1: {e}. \
             cosmic-comp should support this — check you're on a recent COSMIC build."
        ))?;

    let vk = vk_manager.create_virtual_keyboard(&seat, &qh, ());
    install_keymap(&vk)?;

    let mut state = State {
        _seat: seat,
        _vk_manager: vk_manager,
    };

    queue.roundtrip(&mut state)?;

    tracing::info!("virtual keyboard ready");

    loop {
        match rx.recv_timeout(std::time::Duration::from_millis(16)) {
            Ok(cmd) => {
                handle_cmd(&vk, cmd);
                while let Ok(cmd) = rx.try_recv() {
                    handle_cmd(&vk, cmd);
                }
                queue.flush()?;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                queue.dispatch_pending(&mut state)?;
                queue.flush()?;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                tracing::info!("UI dropped sender, shutting down VK thread");
                break Ok(());
            }
        }
    }
}

fn handle_cmd(vk: &ZwpVirtualKeyboardV1, cmd: VkCommand) {
    let time = now_ms();
    match cmd {
        VkCommand::Tap(keycode) => {
            vk.key(time, keycode, 1);
            vk.key(time.wrapping_add(1), keycode, 0);
        }
        VkCommand::Press(keycode) => vk.key(time, keycode, 1),
        VkCommand::Release(keycode) => vk.key(time, keycode, 0),
        VkCommand::Modifiers(mask) => {
            vk.modifiers(mask, 0, 0, 0);
        }
    }
}

fn now_ms() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0)
}

fn install_keymap(vk: &ZwpVirtualKeyboardV1) -> anyhow::Result<()> {
    let ctx = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
    let keymap = xkbcommon::xkb::Keymap::new_from_names(
        &ctx,
        "",
        "",
        "us",
        "",
        None,
        xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .ok_or_else(|| anyhow::anyhow!("failed to compile US xkb keymap"))?;

    let keymap_str = keymap.get_as_string(xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1);
    let bytes = keymap_str.as_bytes();

    let mut tmp = tempfile::tempfile()?;
    tmp.write_all(bytes)?;
    tmp.write_all(&[0u8])?;
    tmp.flush()?;

    vk.keymap(
        xkbcommon::xkb::KEYMAP_FORMAT_TEXT_V1.into(),
        tmp.as_fd(),
        (bytes.len() + 1) as u32,
    );

    Ok(())
}

struct State {
    _seat: WlSeat,
    _vk_manager: ZwpVirtualKeyboardManagerV1,
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _: <ZwpVirtualKeyboardManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _: <ZwpVirtualKeyboardV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

