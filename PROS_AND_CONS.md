# Eden DAW — Pros & Cons

> Honest assessment of Eden's current state as a DAW.
> **Do NOT commit this file.**

---

## Pros

### 🟢 Self-Contained & Portable
- **Zero external dependencies at runtime.** No VST/AU/LV2 host, no plugin scanning, no license servers. Everything is built-in.
- **Single binary.** Just `cargo build --release` and you have a working DAW.
- **Cross-platform SDL2** renders the UI — works on Linux, macOS, and Windows.
- **JSON project files** are human-readable, diff-friendly, and tiny. A complex project is ~50KB.

### 🟢 Full Undo/Redo Stack
- Every single operation (note edit, volume change, clip move, effect add/remove, rack param change) is undoable.
- Snapshot-based: undo always restores the *exact* previous state.
- Redo stack survives until a new command is pushed.

### 🟢 Complete Signal Chain
- Per-track effect rack (instruments → MIDI effects → audio effects → CStrip2 → pan/vol → master bus).
- Master rack with independent effect chain.
- CStrip2 channel strip per track with bypass (A/B comparison).
- Sidechain compression routing.
- √2 constant-power pan law.
- Brick-wall limiter with hard ceiling clamp.

### 🟢 Good Synth Coverage
- **4 synth engines:** Analog (2-osc subtractive), HyperSaw (JP-8000 style), Monolith (heavy + sub + noise + distortion), Sampler.
- Each synth has filter with ADSR envelope, amp ADSR, and dB-based gain.
- MIDI effects: Arpeggiator, Chord, Transpose, Velocity.

### 🟢 Effect Quality
- **13 built-in effects** covering the essentials: LP/HP filter, Delay, Reverb, Chorus, Distortion, Compressor, EQ, Gain, Utility, Limiter, Autoduck, CStrip2.
- Beat-synced delay with 10 division options.
- Reverb with 20 parameters (hall-style with diffuse/modulate/spin/wander).
- All effects have output_db knob for gain staging.

### 🟢 Render Parity
- Offline render (`render_to_buffer`, `render_to_wav_with_progress`) produces output identical to real-time playback.
- CStrip2, effects, pan, volume, master rack all applied in the same order.
- 478 tests verify this with <1e-10 sample-by-sample tolerance.

### 🟢 Multiple Workflow Modes
- **Arrangement** view for timeline editing.
- **Mixer** view with per-track faders and meters.
- **Edit** view with piano roll, audio editor, and automation editor.
- Mode switching via 1/2/3 keys or tabs.

### 🟢 Solid Testing
- 478 automated tests covering: commands, DSP, models, modules, render, parity, save/load, UI state, fuzz.
- Tests are split into themed submodules for maintainability.
- Every serializable field has a round-trip test.

---

## Cons

### 🔴 No Audio Recording
- Cannot record microphone or line input.
- Audio tracks only work with pre-existing WAV files.
- This is the single biggest missing feature for most workflows.

### 🔴 No Plugin Support
- No VST3, AU, LV2, or CLAP support.
- Users are limited to the 4 built-in synths and 13 effects.
- For many professional workflows, this is a dealbreaker.

### 🔴 No MIDI Controller Input
- QWERTY piano keyboard works for auditioning, but:
  - Cannot record from external MIDI keyboards.
  - Cannot map MIDI CC to parameters.
  - Cannot use drum pads, faders, knobs.

### 🔴 No Send/Return or Group Buses
- Every track is independent — no shared effects.
- A reverb on 8 tracks means 8 separate reverb instances.
- No way to group tracks (drums, vocals, etc.) under a bus.

### 🔴 No Clip Copy/Paste
- Cannot Ctrl+C/V clips in the arrangement.
- Must manually recreate patterns or use the limited duplicate mechanism.

### 🔴 No Time-Stretch or Warp
- Audio clips play at their original tempo.
- Cannot fit recorded audio to project BPM.
- No warp markers for alignment.

### 🔴 Linear Automation Only
- No exponential, S-curve, or step automation curves.
- Complex parameter movements require many points.

### 🔴 No Metronome
- No click track for recording or practice.
- Essential for almost every music production workflow.

### 🟡 Large Source Files
- `views.rs` is 23,772 lines — harder to navigate and maintain.
- `tests/mod.rs` is still ~10,445 lines (though now split into submodules).
- `modules.rs` is 5,070 lines.

### 🟡 No Preset System
- Synth and effect settings must be configured from scratch.
- No save/load for instrument patches or effect chains.

### 🟡 Limited Audio Format Support
- Export is WAV only (no FLAC, OGG, MP3).
- Import expects WAV files.

### 🟡 No Undo History Panel
- Can undo/redo but can't see the history stack.
- No way to jump to a specific undo point.

---

## Rating by Use Case

| Use Case | Rating | Explanation |
|----------|--------|-------------|
| Making beats with built-in synths | ⭐⭐⭐⭐ | Solid synths, effects, piano roll |
| Mixing pre-recorded stems | ⭐⭐⭐ | Good mixer, but no buses/sends |
| Recording vocalist/guitar | ⭐ | No recording capability |
| Sound design / ambient | ⭐⭐⭐ | Good effects + automation, but no LFO/freeze |
| Teaching music production | ⭐⭐⭐⭐ | Clean UI, undo, themes, demo project |
| Professional mastering | ⭐⭐ | Basic EQ/compressor/limiter, but no metering or plugin support |
| Live performance | ⭐ | No MIDI input, no real-time features |

---

## Bottom Line

**Eden is a capable, self-contained DAW for writing music with built-in
instruments.** It excels at the "start from scratch, write a beat, export a WAV"
workflow. Its signal chain is correct and well-tested, and its undo/redo system
is unusually complete for a project of this size.

**The main gaps** are audio recording, MIDI input, plugin support, and
send/return routing — features that would take it from "fun tool" to
"production-ready DAW." The good news is that the architecture (per-track rack,
master bus, JSON serialisation) is solid enough to support these additions
incrementally.
