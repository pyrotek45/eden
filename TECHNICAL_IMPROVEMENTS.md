# Eden DAW — Technical Polish Improvements

> Actionable improvements discovered during the UX scenario audit.
> Organised by effort (S/M/L) and grouped by subsystem.
>
> **Do NOT commit this file.**

---

## Signal Chain & Render

### ✅ Already Fixed This Session
- **CStrip2 missing from render paths** — both `render_to_buffer` and `render_to_wav_with_progress` now process CStrip2 per-track.
- **`set_bpm()` missing from `render_to_buffer`** — beat-synced effects (delay, autoduck) now get correct BPM in test render path.
- **√2 pan compensation missing from render** — all 3 render pan locations now apply `* SQRT_2` to match `audio.rs`.
- **`track.volume` applied at wrong stage** — velocity no longer baked with volume; volume now applied at mix stage (matches audio.rs).
- **Audio clip gain × volume** — render now matches audio.rs: `raw * aclip.gain` only (volume applied at pan/mix).

### Remaining Parity Items
| Item | Effort | Notes |
|------|--------|-------|
| Parameter smoothing in render | M | audio.rs uses `smooth_pan`, `smooth_volume`, `smooth_cstrip_params`; render applies values instantly. For a WAV export this is fine, but a render-at-playback-speed mode would need smoothing. |
| Preview path sync | S | audio.rs has a "preview sync" block that applies master rack to preview signal. Render doesn't support preview. No action needed unless preview-render is added. |

---

## UI / UX

| Improvement | Effort | Details |
|-------------|--------|---------|
| Tooltip on knob hover | M | Show param ID, name, current value, min/max. Could use the `ParamDesc` data already available from `get_param_descs()`. |
| Clip clipboard (Ctrl+C/V) | M | Add `clipboard: Option<Vec<Clip>>` to AppState. Copy = clone selected clips; Paste = insert at cursor. |
| Duplicate track | S | Clone Track struct including rack, clips, CStrip2; assign new ID via `project.next_track_id()`. |
| Scale highlighting in piano roll | S | Add `scale: Option<(u8, ScaleType)>` to AppState. Shade rows for notes outside the scale. |
| Undo history panel | M | Show `command_manager.undo_stack` descriptions in a scrollable list in the bottom panel. Click to jump to that state. |
| Metronome | M | Generate click track in audio callback (sine burst at beat boundaries). Add toggle + volume in transport bar. |
| Marker system | M | `Vec<Marker> { beat: f64, name: String, color: [u8;4] }` in Project. Draw in ruler, click to navigate. |

---

## Serialisation / File I/O

| Improvement | Effort | Details |
|-------------|--------|---------|
| Autosave interval config | S | Currently autosaves to `~/.eden/autosave.json`. Add configurable interval (e.g., 30s / 1min / 5min) in config. |
| Export formats (FLAC, OGG) | M | `render_to_wav_with_progress` writes WAV. Add hound/ogg-vorbis support. |
| Stem export | M | Render each track solo'd to separate WAV files. |
| Project templates | S | Ship 3-4 template JSON files (Empty, Beat, Ambient, Full Demo). Show in New Project popup. |
| Effect/instrument presets | M | Save RackSlot to JSON file; browse/load in rack UI. Store in `~/.eden/presets/`. |

---

## Audio Engine

| Improvement | Effort | Details |
|-------------|--------|---------|
| Audio recording | L | Need cpal input stream, ring buffer, recording state in transport, write to WAV on stop. |
| MIDI controller input | L | Use `midir` crate for MIDI input. Route CC to params, notes to instruments. |
| Send/return buses | L | Add `Bus` type: mix output of selected tracks, apply shared effects, mix back. Major audio graph change. |
| Freeze/bounce track | M | Render a single track offline → replace with audio clip. Save CPU. |
| Metronome in audio callback | M | Emit click at beat boundaries in `audio_callback`. |

---

## DSP / Effects

| Improvement | Effort | Details |
|-------------|--------|---------|
| Automation curve types | M | Currently linear-only (`AutomationPoint`). Add `curve_type: CurveType` enum (Linear, Exponential, Step, SCurve). |
| LFO modulation source | M | New "LFO" pseudo-effect that modulates another param at a rate. |
| Time-stretch | L | Pitch-independent time-stretch for audio clips. Would need granular or phase-vocoder algorithm. |
| Crossfade between clips | S | When two clips overlap, blend with a short crossfade (like the existing fade_in/fade_out). |

---

## Testing

### ✅ Already Done This Session
- **9 parity tests** covering CStrip2, bypass, determinism, signal chain order, velocity, √2 pan, no-slew/tanh.
- **11 save/load tests** covering enabled flag, param name/default, render roundtrip, loop region, transport, cstrip2, master rack, multiple clips, mixed types, all track types, midi effects.
- **Test file split** — `tests/mod.rs` + `tests/parity.rs` + `tests/save_load.rs`.

### Remaining Test Gaps
| Test Category | Effort | Details |
|---------------|--------|---------|
| Fuzz: random project render | S | Generate random projects (random tracks, effects, notes) and verify no panics/NaN/Inf. Already have 9 fuzz tests but could expand. |
| Automation render parity | M | Verify automation clips modify rendered output correctly (currently no specific automation render test). |
| Audio clip render parity | M | Test that audio clips with fade_in/fade_out/gain render identically in both paths. |
| Sidechain compression test | M | Verify sidechain source track affects compression behaviour in render. |
| MIDI effect render test | S | Verify arpeggiator/chord/transpose produce expected note patterns in render. |

---

## Code Quality

| Improvement | Effort | Details |
|-------------|--------|---------|
| Extract `draw_*` functions from views.rs | L | views.rs is 23K lines. Could split into `views/arrangement.rs`, `views/mixer.rs`, `views/piano_roll.rs`, etc. |
| Extract audio callback | M | audio.rs callback is ~2800 lines. Could split into `audio/playback.rs`, `audio/metering.rs`. |
| Type-safe param IDs | M | Replace `String` param IDs with an enum to prevent typos (e.g., `"ceiling"` vs `"ceiling_db"`). |
| Reduce `clone()` in snapshot undo | S | Currently clones entire Project for each undo. Could use structural sharing or diff-based undo. |

---

## Summary: Top 10 Polish Items (by effort × impact)

1. **Clip copy/paste** (M effort, high impact)
2. **Tooltip on hover** (M effort, high discoverability)
3. **Duplicate track** (S effort, high convenience)
4. **Metronome** (M effort, essential for recording)
5. **Crossfade between clips** (S effort, audio quality)
6. **Scale highlighting** (S effort, piano roll usability)
7. **Effect presets** (M effort, workflow speed)
8. **Markers** (M effort, navigation)
9. **Stem export** (M effort, professional workflow)
10. **Project templates** (S effort, onboarding)
