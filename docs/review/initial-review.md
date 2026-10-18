# Eden review and repair plan

Reviewed master at `0fc35af9d76e58b025e2efc017af706e6a6a033e` on October 18, 2026. This initial inspection left application files unchanged; the follow-up report records the subsequent UI fixes. Review evidence is preserved in this directory.

## What works

- Rust 1.85.1 / SDL2 debug build succeeds with the existing lockfile.
- `cargo test --locked`: **560 passed, 0 failed, 0 ignored**, 41.84 seconds. Full output: [initial test output](evidence/initial-tests.log).
- Created a project, added a MIDI track and clip, entered notes, saved JSON, restarted, and reloaded that project.
- Arrangement, piano roll, mixer, instrument rack, help, and options render on a virtual X11 display.
- Playback advances and produces a visible waveform using an ALSA null sink. This does not validate audible quality, hardware latency, or device switching.
- Installation helper was rerun successfully. A new published task has not been tested.

## Screenshots

These are actual app captures, not mockups. The initial startup screenshot includes the virtual desktop border; other main views are window captures at 1280×800.

| Screen | Evidence |
|---|---|
| Startup | [Startup](screenshots/01-startup.png) |
| Fresh arrangement | [Empty project](screenshots/03-arrangement.png) |
| Initial mixer size | [Short mixer panel](screenshots/04-mixer.png) |
| MIDI clip | [Arrangement](screenshots/06-midi.png) |
| Help | [Help and missing glyphs](screenshots/07-help.png) |
| Piano roll | [Expanded editor](screenshots/10-piano-roll.png) |
| Mixer | [Expanded mixer](screenshots/12-mixer.png) |
| Instrument rack | [Analog synth](screenshots/13-rack.png) |
| Options | [Options](screenshots/14-options.png) |
| Playback | [Moving playhead and waveform](screenshots/16-playback.png) |

## Findings

### P0 — Protect projects and source media first

**1. Opening another project retains the old undo history.** Source-confirmed: `src/app/state.rs:1115` replaces `self.project` without resetting `self.commands`, audio undo/redo stacks, or several editor selections. `CommandManager::undo` replaces the entire project with its stored snapshot (`src/app/commands.rs:44`). The project-manager load callers do not clear history either. Editing A, opening B, then undoing can restore A into B's active save context. This specific multi-project sequence was not reproduced in the UI. Fix by making project switching a single operation that stops playback, resets all histories and transient selections, and establishes a fresh save context. Acceptance: undo after opening B cannot restore anything from A; failed loads preserve A.

**2. Saves overwrite the destination directly; exit ignores failure.** Source-confirmed: `state.rs:1093` and `:1107` use `std::fs::write` directly. A crash, full disk, or interrupted write can leave an existing project truncated. `main.rs:2072` discards the save result and prints an auto-save success message anyway. Fix with same-directory temporary writes, flush/sync, atomic replacement, recoverable backups, and explicit failure handling. Add Save / Discard / Cancel for dirty-project transitions rather than silently overwriting on exit. Acceptance: simulated write/rename failure preserves the last good file and shows an error; cancelling a close preserves the session.

**3. Audio editing overwrites imported files.** Source-confirmed: normalize, trim, and cut in `src/views/audio_editor.rs` write back to `source_file`; backups are made beside it. A manual clone action exists, but editing remains destructive by default. Shared clips and other projects referencing that file can change too. No real user samples were modified during review. Fix by copying imported media into project-managed storage before edits, or using non-destructive operations; retain a clearly marked opt-in destructive mode. Acceptance: editing an imported clip leaves the original file byte-identical, and undo/redo survives project switching safely.

### P1 — Make audio and timing dependable

**4. Audio startup can fail silently, and the output selector is not wired to the engine.** Reproduced without hardware: logs announce `Engine started`, then report an ALSA config failure. The arrangement remains available without a persistent audio-failure indication. `start_audio_engine` spawns initialization and returns before it succeeds (`src/engine/audio.rs:463`). Options sets `audio_device_changed`, but there is no consumer in the engine/main loop; initialization always uses `default_output_device()`. “Reset Audio Engine” only sets the panic flag, so it cannot recreate a failed stream. Fix with startup acknowledgement, visible device/error state, actual device selection and stream recreation, and distinction between panic and restart. Acceptance: unavailable devices visibly fail; switching output creates the selected stream; restart recovers after a device becomes available.

**5. The audio callback allocates and clones project-sized data.** Source-confirmed: `AudioShared` contains nested vectors and strings; callback lines 600–656 clone it, often twice, while holding the shared mutex. `try_lock` avoids waiting for a mutex but does not prevent allocator delays or expensive work. This contradicts the stronger lock-free/preallocation claims in `docs/audio-engine-notes.md`. Dropouts were not measured here. Fix by publishing prepared immutable render state outside the callback, using bounded event queues, and moving module construction and unbounded allocation off the audio thread. Acceptance: allocator instrumentation shows no steady-state callback allocation/deallocation; stress sessions meet buffer deadlines while editing.

**6. Tempo-map playback/export uses the first BPM.** Source-confirmed: real-time snapshots assign `bpm_at(0.0)` in `main.rs:1686`; both render paths obtain the initial BPM and advance by a constant beats-per-sample (`engine/render.rs:56`, `:254`). The UI separately consults BPM at the current position. Projects with later tempo changes can therefore display one tempo while playing/exporting another. Fix by sharing a beat/time conversion and tempo-event scheduler across transport, playback, and export. Acceptance: a 120→60 BPM project has measured note onsets and duration matching its tempo map, including seek and loop playback.

### P2 — Improve UI clarity before adding features

**7. Text and symbols need a consistent rendering system.** Observed in startup/help/options: unsupported Unicode becomes boxes; the tiny pixel font makes letters and numbers difficult to distinguish; secondary text has weak contrast. `widgets/pixel_label.rs` uses a small custom glyph table and clips strings at the right boundary. Preserve the visual character if desired, but use readable text sizes, a complete icon set, consistent units and accessible contrast. Acceptance: no missing-glyph boxes, clear 100%/125%/150% layouts, and readable labels without hover.

**8. Panel sizing and onboarding obscure the main workflow.** Observed: opening the mixer initially leaves only a narrow strip of content; it requires manually dragging the separator before meters and controls are usable. Even expanded, the mixer shows only part of the channel-strip controls at this height. The empty arrangement provides little guidance beyond Add Track; abbreviations and unlabeled icons increase the learning burden. Fix sensible minimum heights, explicit expand controls, persistent panel layout, scrolling where necessary, and an empty-state sequence: Add track → Choose sound → Create clip → Draw notes → Play. Acceptance: a fresh user can make and play a four-bar phrase without opening help; critical controls remain reachable at supported window sizes.

**9. Preference persistence needs platform-aware paths.** Observed in this restricted cloud machine: saving config fails because Eden writes to `$HOME/.config/eden` even when `XDG_CONFIG_HOME` is supplied (`app/config.rs:115`). This is an environment-specific failure, not proof that normal Linux installations cannot save settings. Fix XDG-aware path resolution and visible persistence errors. Acceptance: a writable XDG config directory works when the home directory is read-only.

## Implementation order

| Phase | Deliverable | Completion gate |
|---|---|---|
| 1. Session safety | Atomic saves/recovery, dirty-transition prompts, clean project-switch state, safe media edits | Failure-injection and cross-project undo tests; original media unchanged |
| 2. Audio lifecycle | Real startup result, device selection, restart/recovery, visible errors | Device-failure and reconnection checks on hardware |
| 3. Timing and DSP reliability | Shared tempo scheduling, callback allocation removal, export/playback comparison | Tempo-change render fixtures, callback deadline measurements, audible playback/export comparison |
| 4. UI usability | Typography/icons/contrast, panel defaults and scrolling, clear empty states, preference paths | Screenshots at several window sizes/scales and a complete create-save-reopen-export user walkthrough |
| 5. Regression coverage and documentation | Add targeted tests for these defects and refresh setup/audio architecture docs | CI runs existing 560 tests plus new behavior checks; docs match implementation |

Keep these as focused changes with their own acceptance checks. Do not start with a large UI rewrite or more instruments: save safety and audio lifecycle defects affect trust in every other feature.

## Scope limits

This is an initial functional and source review, not an exhaustive audit of every effect or editing operation. Real sound, latency, MIDI hardware, Windows/macOS behavior, large-session performance, and complete export/playback equivalence remain unverified. Passing tests establish the current suite's behavior, not the absence of the defects above. The parity suite exercises render helpers and effects; it is not a recording of the actual device callback.

The cloud machine supports building, testing, screen capture and silent playback. Physical audio is unavailable. Preferences cannot persist under the current read-only home policy. Reusable installation/startup instructions are saved separately in the environment draft; publication remains a user action.


Follow-up clarification: deeper inspection found that TempoMap::bpm_at itself ignores the requested beat, so later tempo changes are unsupported throughout, rather than already working in the UI. Changing base BPM also rescales MIDI and automation. See ../tempo-and-clips.md and findings-and-plan.md for the corrected diagnosis, reproduced clip defects and subsequent user-authorized UI fixes.
