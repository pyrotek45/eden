# Eden DAW

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](https://www.gnu.org/licenses/gpl-3.0)

A minimal digital audio workstation built with Rust and SDL2.

> **Note:** This project is built collaboratively by **pyrotek45** and **AI** (GitHub Copilot). It's a fun experiment in human-AI collaboration for creative software development.

## Features

- **Arrangement View** — Timeline-based clip arrangement with drag, resize, loop, and join
- **Piano Roll** — MIDI note editor with velocity editing, snap-to-grid, and computer keyboard input
- **Audio Editor** — Waveform view with selection, normalize, trim, cut, paste, fade, reverse, and export
- **Mixer** — Per-track volume, pan, mute/solo, VU meters, and inline CStrip2 channel strip
- **Built-in Synths** — Analog, HyperSaw, Sampler, and Monolith instruments with ADSR envelopes
- **Effects** — LP Filter, HP Filter, Delay, Reverb, Chorus, Distortion, Compressor, EQ, Gain, Utility, Limiter, Autoduck, CStrip2
- **MIDI Effects** — Arpeggiator, Chord, Transpose, Velocity
- **Automation** — Per-parameter automation lanes with linear and stepped curves
- **Audio Support** — WAV and OGG/Vorbis file import
- **MIDI Import** — Load MIDI files directly into tracks
- **Offline Render** — Export your project to WAV
- **Undo/Redo** — Snapshot-based full history support for every action
- **Theming** — Multiple colour themes (press T to cycle)
- **Master Bus** — Master rack with unlimited effects chain and per-effect bypass

## Screenshots

*Coming soon*

## Building

### Requirements

- Rust (stable, 2021 edition)
- SDL2, SDL2_gfx, SDL2_ttf, SDL2_image development libraries
- ALSA development libraries (Linux)
- pkg-config

### Quick Start (any distro)

Build scripts are provided in `scripts/` for all major Linux distributions:

```bash
# Pick the script for your distro — it will install deps and build:
./scripts/build-arch.sh      # Arch / Manjaro
./scripts/build-debian.sh    # Debian / Ubuntu / Mint
./scripts/build-fedora.sh    # Fedora / RHEL / CentOS
./scripts/build-void.sh      # Void Linux
./scripts/build-nixos.sh     # NixOS (uses shell.nix)
```

### Manual Build

Install dependencies for your distro, then:

```bash
# Arch / Manjaro
sudo pacman -S rust sdl2 sdl2_gfx sdl2_ttf sdl2_image alsa-lib pkg-config

# Debian / Ubuntu
sudo apt install rustc cargo pkg-config libsdl2-dev libsdl2-gfx-dev libsdl2-ttf-dev libsdl2-image-dev libasound2-dev

# Fedora
sudo dnf install rust cargo pkg-config SDL2-devel SDL2_gfx-devel SDL2_ttf-devel SDL2_image-devel alsa-lib-devel

# Void Linux
sudo xbps-install -S rust cargo pkg-config SDL2-devel SDL2_gfx-devel SDL2_ttf-devel SDL2_image-devel alsa-lib-devel
```

Then build:

```bash
cargo build --release
```

The binary will be at `target/release/eden`.

### On NixOS

A `shell.nix` is provided with all dependencies:

```bash
nix-shell
cargo build --release
```

### Windows (Cross-Compilation)

Cross-compilation from Linux is supported via MinGW:

```bash
./scripts/build-windows.sh
```

The script will guide you through installing the MinGW toolchain and SDL2 Windows libraries.

## Running

```bash
./target/release/eden
```

Or use `cargo run --release`.

## Packaging for Distribution

A packaging script bundles the binary with SDL2 libraries into a portable tarball:

```bash
# Inside nix-shell (uses patchelf to remove nix store paths)
./scripts/package.sh
```

This creates `dist/eden-linux-x86_64.tar.gz` — a self-contained bundle that runs on any x86_64 Linux with glibc and ALSA.

## Controls

Press **F1** (or **H**) in the app to see the full help screen. Here are the highlights:

| Action | Key/Mouse |
|--------|-----------|
| Play / Stop | Space |
| Stop & Rewind | Enter |
| Toggle Loop | L |
| Save Project | Ctrl+S |
| Undo | Ctrl+Z |
| Redo | Ctrl+Shift+Z |
| Delete | Delete / Backspace |
| Select All | Ctrl+A |
| Copy / Paste | Ctrl+C / Ctrl+V |
| Duplicate | Ctrl+D |
| Pan View | Middle Mouse Drag |
| Zoom | Ctrl+Scroll |
| Horizontal Scroll | Shift+Scroll |
| Arrangement View | 1 |
| Mixer View | 2 |
| Edit / Piano Roll | 3 |
| Snap Toggle | S |
| Cycle Theme | T |
| Help Screen | F1 |

## Project Structure

```
src/
  app/         — Input, commands (undo/redo), state, models, config
  dsp/         — Audio DSP helpers (automation, mixing, voices)
  engine/      — Audio engine, offline render
  modules/     — Instruments, effects, MIDI effects, DSP primitives
  tests/       — Test suites (unit, save/load, DSP parity)
  theme/       — Colour themes
  views/       — All UI views (arrangement, mixer, piano roll, etc.)
  widgets/     — Reusable UI widgets (knobs, sliders, buttons, etc.)
  main.rs      — Application entry point
scripts/       — Build & packaging scripts for all platforms
docs/          — Internal development notes
```

## License

This project is licensed under the **GNU General Public License v3.0** — see the [LICENSE](LICENSE) file for details.

This means:
- ✅ Free to use, modify, and distribute
- ✅ Must keep source code open
- ✅ Must use the same license for derivatives
- ❌ Cannot be made proprietary

## Contributing

Contributions are welcome! Feel free to open issues or submit pull requests.

## Credits

- **pyrotek45** — Human developer
- **GitHub Copilot** — AI pair programmer

Built with ❤️ and 🤖
