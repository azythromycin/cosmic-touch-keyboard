//! Keyboard layouts. Each layer is a Vec<Row>; each row is a Vec<Key>.
//!
//! Keycodes here are **evdev** codes (what `xkb` calls "raw" keycodes, i.e.
//! XKB_keycode - 8). This matches what `zwp_virtual_keyboard_v1::key()` wants.
//! Reference: `/usr/include/linux/input-event-codes.h`.

#[derive(Debug, Clone)]
pub struct Key {
    pub label: String,
    #[allow(dead_code)]
    pub shifted: Option<String>, // rendered when Shift is armed (letters layer)
    pub action: KeyAction,
    /// Width in "units" (1.0 = a standard letter key). Spacebar = 6.0, etc.
    pub width: f32,
}

#[derive(Debug, Clone)]
pub enum KeyAction {
    /// Send this evdev keycode as a tap.
    Code(u32),
    /// Switch active layer.
    Layer(Layer),
    /// Toggle a modifier on/off (sticky).
    Modifier(Mod),
    /// Hide/dismiss the keyboard window.
    #[allow(dead_code)]
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Letters,
    Symbols,
    Emoji,
    #[allow(dead_code)]
    Nav,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mod {
    Shift,
    Ctrl,
    Alt,
    Super,
}

impl Mod {
    /// XKB modifier bitmask (matches xkb_state_serialize_mods output for a US layout).
    pub fn mask_bit(self) -> u32 {
        match self {
            Mod::Shift => 1 << 0,
            Mod::Ctrl => 1 << 2,
            Mod::Alt => 1 << 3,
            Mod::Super => 1 << 6,
        }
    }
}

// -------- evdev keycodes (subset, named for clarity) ----------
// From linux/input-event-codes.h
pub mod kc {
    pub const ESC: u32 = 1;
    pub const N1: u32 = 2;
    pub const N2: u32 = 3;
    pub const N3: u32 = 4;
    pub const N4: u32 = 5;
    pub const N5: u32 = 6;
    pub const N6: u32 = 7;
    pub const N7: u32 = 8;
    pub const N8: u32 = 9;
    pub const N9: u32 = 10;
    pub const N0: u32 = 11;
    pub const MINUS: u32 = 12;
    pub const EQUAL: u32 = 13;
    pub const BACKSPACE: u32 = 14;
    pub const TAB: u32 = 15;
    pub const Q: u32 = 16;
    pub const W: u32 = 17;
    pub const E: u32 = 18;
    pub const R: u32 = 19;
    pub const T: u32 = 20;
    pub const Y: u32 = 21;
    pub const U: u32 = 22;
    pub const I: u32 = 23;
    pub const O: u32 = 24;
    pub const P: u32 = 25;
    pub const LBRACE: u32 = 26;
    pub const RBRACE: u32 = 27;
    pub const ENTER: u32 = 28;
    pub const A: u32 = 30;
    pub const S: u32 = 31;
    pub const D: u32 = 32;
    pub const F: u32 = 33;
    pub const G: u32 = 34;
    pub const H: u32 = 35;
    pub const J: u32 = 36;
    pub const K: u32 = 37;
    pub const L: u32 = 38;
    pub const SEMI: u32 = 39;
    pub const APOS: u32 = 40;
    pub const GRAVE: u32 = 41;
    pub const BACKSLASH: u32 = 43;
    pub const Z: u32 = 44;
    pub const X: u32 = 45;
    pub const C: u32 = 46;
    pub const V: u32 = 47;
    pub const B: u32 = 48;
    pub const N: u32 = 49;
    pub const M: u32 = 50;
    pub const COMMA: u32 = 51;
    pub const DOT: u32 = 52;
    pub const SLASH: u32 = 53;
    pub const SPACE: u32 = 57;
    pub const F1: u32 = 59;
    pub const F2: u32 = 60;
    pub const F3: u32 = 61;
    pub const F4: u32 = 62;
    pub const F5: u32 = 63;
    pub const F6: u32 = 64;
    pub const F7: u32 = 65;
    pub const F8: u32 = 66;
    pub const F9: u32 = 67;
    pub const F10: u32 = 68;
    pub const F11: u32 = 87;
    pub const F12: u32 = 88;
    pub const HOME: u32 = 102;
    pub const UP: u32 = 103;
    pub const PGUP: u32 = 104;
    pub const LEFT: u32 = 105;
    pub const RIGHT: u32 = 106;
    pub const END: u32 = 107;
    pub const DOWN: u32 = 108;
    pub const PGDN: u32 = 109;
    pub const INSERT: u32 = 110;
    pub const DELETE: u32 = 111;
}

// Helpers for building rows.
fn k(label: &str, shifted: &str, code: u32) -> Key {
    Key {
        label: label.into(),
        shifted: Some(shifted.into()),
        action: KeyAction::Code(code),
        width: 1.0,
    }
}
fn sw(label: &str, code: u32, width: f32) -> Key {
    Key {
        label: label.into(),
        shifted: None,
        action: KeyAction::Code(code),
        width,
    }
}
fn m(label: &str, modifier: Mod, width: f32) -> Key {
    Key {
        label: label.into(),
        shifted: None,
        action: KeyAction::Modifier(modifier),
        width,
    }
}
fn l(label: &str, layer: Layer, width: f32) -> Key {
    Key {
        label: label.into(),
        shifted: None,
        action: KeyAction::Layer(layer),
        width,
    }
}

pub fn letters() -> Vec<Vec<Key>> {
    vec![
        // Row 1: numbers
        vec![
            k("1", "!", kc::N1), k("2", "@", kc::N2), k("3", "#", kc::N3),
            k("4", "$", kc::N4), k("5", "%", kc::N5), k("6", "^", kc::N6),
            k("7", "&", kc::N7), k("8", "*", kc::N8), k("9", "(", kc::N9),
            k("0", ")", kc::N0), k("-", "_", kc::MINUS), k("=", "+", kc::EQUAL),
            sw("⌫", kc::BACKSPACE, 1.5),
        ],
        // Row 2: QWERTY top
        vec![
            sw("Tab", kc::TAB, 1.5),
            k("q", "Q", kc::Q), k("w", "W", kc::W), k("e", "E", kc::E),
            k("r", "R", kc::R), k("t", "T", kc::T), k("y", "Y", kc::Y),
            k("u", "U", kc::U), k("i", "I", kc::I), k("o", "O", kc::O),
            k("p", "P", kc::P),
            k("[", "{", kc::LBRACE), k("]", "}", kc::RBRACE),
            k("\\", "|", kc::BACKSLASH),
        ],
        // Row 3: home row
        vec![
            m("Caps", Mod::Shift, 1.75), // doubles as shift-lock here; simple UX
            k("a", "A", kc::A), k("s", "S", kc::S), k("d", "D", kc::D),
            k("f", "F", kc::F), k("g", "G", kc::G), k("h", "H", kc::H),
            k("j", "J", kc::J), k("k", "K", kc::K), k("l", "L", kc::L),
            k(";", ":", kc::SEMI), k("'", "\"", kc::APOS),
            sw("Enter", kc::ENTER, 2.0),
        ],
        // Row 4: bottom
        vec![
            m("⇧", Mod::Shift, 2.25),
            k("z", "Z", kc::Z), k("x", "X", kc::X), k("c", "C", kc::C),
            k("v", "V", kc::V), k("b", "B", kc::B), k("n", "N", kc::N),
            k("m", "M", kc::M),
            k(",", "<", kc::COMMA), k(".", ">", kc::DOT), k("/", "?", kc::SLASH),
            m("⇧", Mod::Shift, 2.0),
        ],
        // Row 5: modifiers + space + layer switches
        vec![
            m("Ctrl", Mod::Ctrl, 1.25),
            m("Super", Mod::Super, 1.25),
            m("Alt", Mod::Alt, 1.25),
            l("?123", Layer::Symbols, 1.25),
            sw("Space", kc::SPACE, 4.0),
            l("😀", Layer::Emoji, 1.25),
            sw("←", kc::LEFT, 1.0),
            sw("↓", kc::DOWN, 1.0),
            sw("↑", kc::UP, 1.0),
            sw("→", kc::RIGHT, 1.0),
        ],
    ]
}

pub fn symbols() -> Vec<Vec<Key>> {
    // A dedicated symbol layer — we can reuse the same evdev codes combined with
    // Shift via the modifier system, but it's cleaner UX to give users a flat
    // layer of every common symbol.
    vec![
        vec![
            k("1", "!", kc::N1), k("2", "@", kc::N2), k("3", "#", kc::N3),
            k("4", "$", kc::N4), k("5", "%", kc::N5), k("6", "^", kc::N6),
            k("7", "&", kc::N7), k("8", "*", kc::N8), k("9", "(", kc::N9),
            k("0", ")", kc::N0),
            sw("⌫", kc::BACKSPACE, 1.5),
        ],
        vec![
            k("`", "~", kc::GRAVE),
            k("-", "_", kc::MINUS),
            k("=", "+", kc::EQUAL),
            k("[", "{", kc::LBRACE),
            k("]", "}", kc::RBRACE),
            k("\\", "|", kc::BACKSLASH),
            k(";", ":", kc::SEMI),
            k("'", "\"", kc::APOS),
            k(",", "<", kc::COMMA),
            k(".", ">", kc::DOT),
            k("/", "?", kc::SLASH),
        ],
        vec![
            m("⇧", Mod::Shift, 1.75),
            // Shift-layer convenience: tapping here emits the shifted glyphs
            // by sending Shift+base. Implemented in app.rs by routing through
            // the current modifier state.
            k("!", "1", kc::N1), k("@", "2", kc::N2), k("#", "3", kc::N3),
            k("$", "4", kc::N4), k("%", "5", kc::N5), k("^", "6", kc::N6),
            k("&", "7", kc::N7), k("*", "8", kc::N8),
            sw("Enter", kc::ENTER, 2.0),
        ],
        vec![
            l("ABC", Layer::Letters, 1.5),
            m("Ctrl", Mod::Ctrl, 1.25),
            m("Alt", Mod::Alt, 1.25),
            sw("Space", kc::SPACE, 4.0),
            l("😀", Layer::Emoji, 1.25),
            sw("←", kc::LEFT, 1.0),
            sw("↓", kc::DOWN, 1.0),
            sw("↑", kc::UP, 1.0),
            sw("→", kc::RIGHT, 1.0),
        ],
    ]
}

pub fn nav() -> Vec<Vec<Key>> {
    vec![
        vec![
            sw("Esc", kc::ESC, 1.0),
            sw("F1", kc::F1, 1.0), sw("F2", kc::F2, 1.0), sw("F3", kc::F3, 1.0),
            sw("F4", kc::F4, 1.0), sw("F5", kc::F5, 1.0), sw("F6", kc::F6, 1.0),
            sw("F7", kc::F7, 1.0), sw("F8", kc::F8, 1.0), sw("F9", kc::F9, 1.0),
            sw("F10", kc::F10, 1.0), sw("F11", kc::F11, 1.0), sw("F12", kc::F12, 1.0),
        ],
        vec![
            sw("Insert", kc::INSERT, 1.5),
            sw("Home", kc::HOME, 1.25),
            sw("PgUp", kc::PGUP, 1.25),
            sw("↑", kc::UP, 1.25),
            sw("PgDn", kc::PGDN, 1.25),
            sw("End", kc::END, 1.25),
            sw("Del", kc::DELETE, 1.5),
        ],
        vec![
            sw("Tab", kc::TAB, 1.5),
            sw("←", kc::LEFT, 1.25),
            sw("↓", kc::DOWN, 1.25),
            sw("→", kc::RIGHT, 1.25),
            sw("Enter", kc::ENTER, 2.0),
            sw("⌫", kc::BACKSPACE, 1.5),
        ],
        vec![
            l("ABC", Layer::Letters, 1.5),
            m("Ctrl", Mod::Ctrl, 1.25),
            m("Super", Mod::Super, 1.25),
            m("Alt", Mod::Alt, 1.25),
            sw("Space", kc::SPACE, 4.0),
            l("?123", Layer::Symbols, 1.25),
        ],
    ]
}

/// Emoji are a special case. The virtual_keyboard protocol sends keycodes,
/// so we can't inject arbitrary Unicode directly through it. However, the US
/// xkb layout we installed *does* support Ctrl+Shift+U sequences on apps that
/// honor IBus/fcitx — but that's fragile.
///
/// The clean approach used here: the emoji layer is a curated set, and each
/// tap injects a sequence of keystrokes via `xdotool`-style text typing. For
/// COSMIC specifically, we instead rely on `wl-clipboard` being available.
/// To keep this self-contained, the current implementation just logs the
/// emoji and emits a placeholder; in production you'd pipe these through
/// `wtype -` (see README).
///
/// For now, emoji keys carry an evdev code of 0 and the app special-cases
/// them by copying the glyph to clipboard + pasting via Ctrl+V.
pub fn emoji() -> Vec<Vec<Key>> {
    let rows = [
        vec!["😀","😃","😄","😁","😆","😅","😂","🤣","😊","😇","🙂","🙃","😉","😌","😍"],
        vec!["🥰","😘","😗","😙","😚","😋","😛","😜","🤪","😝","🤑","🤗","🤭","🤫","🤔"],
        vec!["🤐","🤨","😐","😑","😶","😏","😒","🙄","😬","🤥","😌","😔","😪","🤤","😴"],
        vec!["👍","👎","👌","✌️","🤞","🤟","🤘","👏","🙌","👐","🤲","🤝","🙏","✍️","💪"],
        vec!["❤️","🧡","💛","💚","💙","💜","🖤","🤍","🤎","💔","❣️","💕","💞","💓","💗"],
    ];

    let mut layers: Vec<Vec<Key>> = rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|g| Key {
                    label: (*g).to_string(),
                    shifted: None,
                    action: KeyAction::Code(0), // sentinel; handled specially
                    width: 1.0,
                })
                .collect()
        })
        .collect();

    // Bottom control row
    layers.push(vec![
        l("ABC", Layer::Letters, 1.5),
        sw("Space", kc::SPACE, 6.0),
        sw("⌫", kc::BACKSPACE, 1.5),
        sw("Enter", kc::ENTER, 1.5),
    ]);

    layers
}

pub fn layout_for(layer: Layer) -> Vec<Vec<Key>> {
    match layer {
        Layer::Letters => letters(),
        Layer::Symbols => symbols(),
        Layer::Emoji => emoji(),
        Layer::Nav => nav(),
    }
}
