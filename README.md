# Eden DAW

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

A minimal digital audio workstation built with Rust and SDL2.

> **Note:** This project is built collaboratively by **pyrotek45** and **AI** (GitHub Copilot). It's a fun experiment in human-AI collaboration for creative software development.

## Features

- **Arrangement View** — Timeline-based clip arrangement with drag, resize, and loop support
- **Piano Roll** — MIDI note editor with velocity editing and snap-to-grid
- **Mixer** — Track volume, pan, mute/solo controls
- **Built-in Synths** — Sine, saw, square, triangle oscillators with ADSR envelopes
- **Effects** — Reverb, delay, chorus, distortion, EQ, compressor, limiter, filter, phaser, flanger, bitcrusher, autoduck
- **Audio Support** — WAV and OGG/Vorbis file import
- **MIDI Import** — Load MIDI files directly
- **Offline Render** — Export your project to WAV
- **Undo/Redo** — Full history support
- **Theming** — Multiple color themes

## Screenshots

*Coming soon*

## Building

### Requirements

- Rust (stable)
- SDL2 and SDL2_gfx development libraries
- ALSA development libraries (Linux)

### On NixOS

A `shell.nix` is provided:

```bash
nix-shell
cargo build --release
```

### On Other Linux Distros

Install dependencies first:

```bash
# Arch/Manjaro
sudo pacman -S rust sdl2 sdl2_gfx alsa-lib

# Ubuntu/Debian
sudo apt install rustc cargo libsdl2-dev libsdl2-gfx-dev libasound2-dev

# Fedora
sudo dnf install rust cargo SDL2-devel SDL2_gfx-devel alsa-lib-devel
```

Then build:

```bash
cargo build --release
```

The binary will be at `target/release/eden`.

## Running

```bash
./target/release/eden
```

Or use `cargo run --release`.

## Packaging for Distribution

A packaging script is provided for creating distributable Linux binaries:

```bash
# Inside nix-shell
./scripts/package.sh
```

This creates `dist/eden-linux-x86_64.tar.gz` with bundled libraries.

## Controls

Press **H** in the app to see the full help screen. Here are the highlights:

| Action | Key/Mouse |
|--------|-----------|
| Play/Stop | Space |
| New Project | Ctrl+N |
| Open Project | Ctrl+O |
| Save Project | Ctrl+S |
| Undo | Ctrl+Z |
| Redo | Ctrl+Y |
| Delete | Delete/Backspace |
| Pan View | Middle Mouse Drag |
| Zoom | Ctrl+Scroll |
| Horizontal Scroll | Shift+Scroll |

## License

This project is licensed under the **GNU General Public License v3.0** — see the [LICENSE](LICENSE) file for details.

This means:
- ✅ Free to use, modify, and distribute
- ✅ Must keep source code open
- ✅ Must use the same license for derivatives
- ❌ Cannot be made proprietary
- ❌ Cannot charge for the software itself (though you can charge for services)

## Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## Credits

- **pyrotek45** — Human developer
- **GitHub Copilot** — AI pair programmer

Built with ❤️ and 🤖
