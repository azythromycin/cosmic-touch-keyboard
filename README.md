# cosmic-touch-keyboard

On-screen keyboard for Wayland with a bottom panel UI and touch-focused behavior.

## Install from scratch

```bash
sudo apt update
sudo apt install build-essential pkg-config libxkbcommon-dev wtype wl-clipboard
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
git clone <your-repo-url>
cd cosmic-touch-keyboard
cargo build --release
sudo install -Dm755 target/release/cosmic-touch-keyboard /usr/local/bin/cosmic-touch-keyboard
```

## Run

```bash
cosmic-touch-keyboard
```

## Start on login

```bash
mkdir -p ~/.config/autostart
cp cosmic-touch-keyboard.desktop ~/.config/autostart/cosmic-touch-keyboard.desktop
```

If needed, edit `Exec=` in `~/.config/autostart/cosmic-touch-keyboard.desktop` to:

```ini
Exec=/usr/local/bin/cosmic-touch-keyboard
```

## Debian package

```bash
make package
sudo apt install ./dist/cosmic-touch-keyboard_*_*.deb
```

## Quick checks

```bash
wayland-info | rg virtual_keyboard
wayland-info | rg input_method
```

## Requirements

- Wayland compositor with `zwp_virtual_keyboard_manager_v1`
- `zwp_input_method_manager_v2` for focus-based show and hide

## License

MPL-2.0
