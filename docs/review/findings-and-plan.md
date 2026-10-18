# Eden: clips, audio memory, rack and mixer

Review of master `0fc35af9d76e58b025e2efc017af706e6a6a033e`, October 18, 2026. The diagnostic review initially left application files unchanged; subsequent user-authorized UI fixes are listed in the follow-up below. This supplements the [initial review](initial-review.md).

The main problem is that clip interaction, content ownership, audio scheduling and drawing are tightly connected. Cosmetic improvements will help, but predictable clip behavior needs a clearer model underneath.

## Verified behavior

I built an external diagnostic executable that imports Eden's actual modules and calls the actual private drag handler through a thin wrapper. It synthesizes successive input frames, including release, without changing repository source or tests. The stereo probe uses the real audio loader, and allocation counts use Rust's allocator around the real `AudioShared::clone`. These are focused handler/DSP probes, not full mouse-to-screen automation or a hardware performance benchmark. [Output](evidence/probe-results.txt), [probe source](probes/probe.rs.in), [rerun script](probes/run.py).

| Finding | Actual result | Intended result |
|---|---|---|
| Right-edge resize undo | A 4-beat clip becomes 5 beats; undo leaves it at 5 | Restore 4 |
| Group left trim | Clips of 4 and 6 beats become 3 and 5, then 2 and 4 on another frame at the same pointer position | Stay at 3 and 5 |
| Shift stretch | Length doubles to 8 beats; a note stays at beat 1 with length 0.5 | A true MIDI stretch would move it to beat 2 with length 1 |
| Invalid copy-drag | Target marked invalid, but an audio track receives a MIDI clip | Reject the drop |
| Group move against beat zero | Two clips 8 beats apart both end up at zero | Clamp the whole group's movement, preserving spacing |
| Stereo import/playback loader | L=+0.5, R=-0.5 becomes all-zero mono | Preserve the two channels |
| Audio snapshot clone | 100 MIDI clips / 3,200 notes: **106 allocations, 83,116 requested bytes** for one clone | No project-sized clone/allocation in the audio callback |

The existing 560 tests passed in the earlier review. These new probes expose behavior those tests do not cover; they are deliberately reporting failures rather than turning them into passing assertions.

## 1. Resize, stretch and looping are conflated

`src/views/track_lanes.rs:1100` draws a TIME STRETCH indicator when Shift is held. The handler at `:2098` does not branch into a stretch operation: it updates clip length. `AudioClip` has source path, source offset, timeline length and fades, but no playback-rate or warp mapping (`app/models.rs:37`). `dsp/mixing.rs:30` advances the source at native speed.

Consequences:

- Right-edge audio resize trims or reveals more of the existing recording; it cannot stretch the recording.
- MIDI resize changes the boundary but does not scale notes.
- No separate clip-level loop region exists in these clip models. Making a clip longer is not a defined repeat operation.
- Right-edge audio limits depend on whether the UI waveform cache has loaded metadata (`track_lanes.rs:2123`). Edit validity should not depend on what has been drawn.
- MIDI left-edge editing updates the timeline start without a separate content offset or compensating note shift. It therefore lacks a clean distinction between trimming content and moving it.

**Solution:** define separate operations, each with a clear cursor and feedback:

| Operation | Contract |
|---|---|
| Move | Change arrangement position; leave source mapping alone |
| Trim | Change visible boundaries and adjust source/content offset; preserve remaining event positions in the song |
| Slip | Move source content within fixed arrangement boundaries |
| Loop | Extend arrangement duration by repeating a defined content region |
| Stretch | Change mapping of source time to arrangement time; scale MIDI events or process audio |
| Duplicate | New clip identity at the next position, preserving metadata and relative group spacing |
| Consolidate | Explicitly render selected regions into a new managed asset |

For audio, distinguish varispeed (duration and pitch change) from pitch-preserving stretch. Start with an explicit, quality-labeled offline stretch job into a new asset and cache the result. Choose a production DSP backend only after checking licensing, supported targets, transients, latency and cancellation behavior. Add real-time warp markers later if needed. Do not rename the current native-speed trim operation as stretching.

## 2. Drag handling mutates the document before a reliable undo transaction exists

Single right-resize changes `project` continuously. On release, `CommandManager::execute` snapshots the already-resized project, so undo restores the resized state. The left-resize path explicitly restores state first, showing the inconsistency (`track_lanes.rs:1975` versus `:2181`).

Group left-resize computes its end from original start plus the **current** length (`:2053`), repeatedly applying the same delta. Multi-audio left-resize also lacks the per-clip source-offset updates present in the single-clip branch. Group movement clamps each clip independently at zero (`:1664`), and copy-drag builds additions without enforcing the destination-valid flag (`:1740`). Mixed-validity cross-track moves can be partial rather than atomic.

**Solution:** capture a complete immutable `DragSession` on pointer-down: stable IDs, original start/length, source offsets, note/content mapping, selection, destination constraints and edit mode. Each frame calculates a preview from that baseline. Validate the entire group, clamp one shared delta, and apply one transaction on release. Escape discards the preview. A preview should not mutate the saved project or feed provisional changes into the audio engine unless deliberate live audition is supported.

This is a focused refactor that can precede the larger audio model migration. Add regression tests for these exact sequences, repeated stationary frames, clamp boundaries, release without motion, Escape, snap, mixed types and undo/redo.

## 3. Duplication and content identity need explicit semantics

Ctrl+D already duplicates by the selected group's total span; that is a useful behavior to retain (`main.rs:1030`). MIDI duplication deep-copies note vectors; audio duplication copies the source path and reuses cached PCM through `Arc`. Audio duplication does **not** inherently duplicate the full sample buffer, which is worth preserving.

However, clips are identified throughout editing by `(track_id, vector_index)`, and clip-library sync treats name plus type as identity (`app/state.rs:1072`). Deleting/reordering shifts indices; same-name clips can be conflated; library entries are cloned snapshots rather than clearly defined linked content.

**Solution:** give clips persistent `ClipId`s, content persistent `ContentId`s, and media `AssetId`s. Normal duplication creates a new clip ID and independent editable MIDI content using copy-on-write storage; audio copies reference the same immutable asset. Offer linked MIDI copies as an explicit later feature. A library entry should intentionally be either a snapshot preset or a link, with clear update behavior. Route keyboard, drag, clipboard and library operations through the same editing service.

Define overlaps too: audio currently sums overlapping clips in `mix_audio_clips`. Decide whether a move permits layering, replaces a region, or shows a conflict; expose that choice and test it. Avoid silently destructive replacement.

## 4. Audio ownership has both correctness and memory problems

### Stereo is lost in the playback path

`engine/audio.rs:298` and `:334` average input channels into mono; `main.rs:1595` uses this loader for arrangement clips. `dsp/mixing.rs:82` sends the same sample to left and right. The editor separately loads interleaved stereo, so a stereo-looking waveform can play as mono. The shared offline audio-cache path also uses the mono loader (`dsp/track_setup.rs`).

Keep channel-aware immutable PCM, preserve stereo through playback and export, and make any mono conversion an explicit operation. Validate with left-only/right-only, opposite-phase stereo and ordinary stereo fixtures. Stereo pan should be deliberately defined as balance or dual-channel pan rather than treating every source as a mono point.

### Decoding and caching lack a common lifecycle

There is a playback `Arc<Vec<f32>>` cache in `main.rs`, plus peak, stereo-peak and raw stereo caches in `AppState`. Peak loading can decode the full file separately from playback and editor loading. Decode and waveform work run on the UI thread; “one file per frame” still allows a long file to freeze a frame. These caches have no general byte-budget eviction or project-switch cleanup; explicit removals mostly handle edited-file invalidation. Failed playback decodes can be cached as empty audio and then silently skipped.

A 10-minute 44.1 kHz recording uses about **303 MiB** for one mono playback buffer plus two full-resolution editor channel buffers alone, excluding transient decode buffers, peak arrays, history and additional files. This is an estimate from the stored f32 layouts, not a measured process RSS. Deleting the clip does not guarantee that cached data is released.

### History and callback work scale with project size

`CommandManager` stores up to 1,000 whole-project snapshots. Those snapshots clone note vectors and metadata; they do not contain the playback PCM cache, so this is not 1,000 copies of every WAV. Audio-editor undo separately creates disk backups and can retain project snapshots. Main rebuilds playback track descriptions every frame, and callback snapshot cloning allocates nested vectors/strings, often twice (`engine/audio.rs:649`). The probe's 106 allocations are for **one clone**, not the entire callback.

**Solution:**

- An `AssetStore` owns immutable channel-aware decoded data and metadata, with stable IDs and revisioned derived assets.
- Decode, waveform generation, stretch and consolidation run as cancellable worker jobs with visible progress/errors.
- Build peaks from the same decoded source; keep multiresolution peaks instead of independent full-file decodes.
- Pin active playback assets. Evict unused data with a byte-budget LRU; account for undo references and editor use. Stream long recordings later if measurements justify it.
- Publish prepared playback state only when relevant revisions change. Send control events through bounded queues; reclaim retired state away from the audio callback. An `Arc` alone does not guarantee that its last-reference deallocation happens safely off the callback.
- Use compact edit transactions or persistent/copy-on-write project data for undo, with memory and disk budgets plus recovery-safe cleanup.

## 5. A slimmer mixer is already partly implemented

`views/mixer.rs:178` defines **260 px full strips** and **80 px slim strips**. The header collapse button toggles `mixer_slim_tracks`; slim mode hides the VU gauge and CStrip2 section. At 1280 px width, a 220 px master and 60 px preview strip leave roughly 988 px, enough for only about three full strips before scrolling.

The full strip combines level mixing, an analog meter, EQ/comp controls and curves. Controls stop drawing when they do not fit (`:618`). It is trying to be both a mixer and a channel-strip editor. Also, drawing initializes missing CStrip2 parameters (`:599`), so merely rendering UI can mutate project data.

**Recommended direction:** make 64–88 px strips the default: track name/color, compact stereo meter, fader and dB readout, pan, mute/solo, and a device-chain button. Keep a compact master; move LUFS, scope and other diagnostics to an expandable master panel. Hide sample-preview metering unless preview is active. Use the existing slim mode as a low-risk first step.

Move CStrip2 into the selected track's device view as a collapsible device. Preserve existing sound: migrate its parameters, bypass state and automation bindings into the chain at its current position **after other track effects and before pan/fader**. Do not delete its DSP or apply it twice. Old projects should render the same before and after migration.

## 6. Why the rack does not yet feel like Ableton

The current horizontal chain is a reasonable starting point. But it sizes device width from parameter count, with 80 px cells, four generic rows and a minimum 300 px height (`mixer.rs:1912–1974`). All controls compete at equal visual weight. Device-specific visualizers are embedded in the same large renderer. Short panels clip rather than providing a coherent compact view. Device view and master view have separate large render paths.

Use Ableton's hierarchy as inspiration:

- One shared device view follows the selected track or master; clip view is its sibling, not another permanent pile of panels.
- Each device has a consistent header: enable, name, selection, fold, menu. Reordering has a clear insertion marker; incompatible drops are rejected.
- Devices have a compact default state, a narrow folded state, and an optional expanded editor.
- Instruments group related controls: oscillator, filter, envelope, modulation. Keep important values visible in meaningful units; use tabs or disclosure for secondary controls.
- Effects use semantic layouts: an EQ graph with a selected band's controls, an envelope curve with ADSR, a sampler waveform with source/loop boundaries. Avoid generic walls of equal knobs.
- Reserve rack macros and parallel chains for a later milestone. A good horizontal device chain does not require implementing Ableton's entire rack feature set.
- Describe supported signal stages truthfully. The engine currently gathers MIDI effects, the first enabled instrument, then audio effects; arbitrary visual device order is not an arbitrary audio graph. Show typed MIDI → instrument → audio sections and validate moves, unless the engine is deliberately expanded.

See the [layout proposal](layout-proposal.html). It is a schematic design proposal, not a screenshot or an implemented UI.

## Staged plan

| Stage | Scope | Acceptance gate |
|---|---|---|
| 1. Make existing edits trustworthy | Immutable drag baseline; atomic validation; resize undo; stationary-frame trim; group boundaries; typed drops; remove misleading stretch label | The reproduced cases above pass; one gesture is one undo step; Escape restores original |
| 2. Slim the UI without changing sound | Default slim mixer; shared device shell; collapsible CStrip2 editor; semantic Analog layout | More channels visible; all controls reachable; existing projects render identically |
| 3. Introduce IDs and non-destructive content | Versioned migration; stable IDs; source windows; shared audio assets; consistent duplicate/split/move/trim | Save/reopen old and new projects; duplicated audio shares storage; independent MIDI edits behave predictably |
| 4. Repair the media pipeline | Stereo playback/export; worker decoding; shared peaks; errors; byte budgets; immutable revisions for edits | Stereo fixtures survive; editor stays responsive; unused memory becomes reclaimable |
| 5. Add actual stretch and looping | Explicit loop/source ranges; MIDI scaling; offline audio stretch with cancellation and cache | Measured duration, onset and pitch behavior matches the chosen mode; source file unchanged |
| 6. Remove callback allocation and optimize history | Revisioned engine state, bounded event delivery, off-thread reclamation, bounded history | Allocation instrumentation and realistic sessions meet audio deadlines; repeat imports/edits/deletes reach a stable memory budget |

Stage 6 can be developed alongside stages 3–5 once the ownership interfaces are settled. Do not defer stereo correctness until the end. Each stage should be several focused changes with regression checks, rather than one rewrite of the whole DAW.

## Boundaries of this review

The deep clip/audio defects remain unfixed; no user audio was edited, and no hardware dropouts or latency numbers were measured. The handler probes reproduce the logic failures directly, but complete GUI integration checks are still needed after fixes. A single architecture change will not automatically solve all of these issues: editing transactions, immutable media ownership and UI hierarchy each need their own completion checks.


## Follow-up: requested UI fixes and ongoing project rules

Added repository `AGENTS.md`, making noticed UI defects part of task completion, requiring enabled controls to perform real actions, real screenshot review and explicit timing/audio contracts. Added `docs/tempo-and-clips.md` with the four coordinate systems, expected DAW tempo behavior, source mapping, meter handling and a migration/test plan. The HTML proposal includes a tempo-versus-zoom illustration.

Changes now implemented separately from the clip/audio redesign:

- Toolbar actions size to their labels and wrap to additional rows instead of overlapping transport controls. Oscilloscope uses only genuinely available space; keyboard octave controls participate in layout.
- BPM/POS fields no longer overlap. Small headers sit above fields, and GRID has its own caption. Snap explicitly displays ON/OFF.
- Record was confirmed to have no recording implementation anywhere in the input path. It is now visibly disabled with an explanatory tooltip; recording has not been implemented.
- Toolbar and keyboard undo/redo use the same focused-editor action, including the audio editor's independent file history; successful actions mark the project dirty. Unavailable undo/redo controls are disabled. Ctrl+Y now matches the toolbar hint.
- Failed audio history restores retain recovery entries instead of popping them before success. This is not a substitute for the broader atomic file-save/media-edit redesign.
- Save failures from the toolbar are surfaced in the application's status display.
- Mixer defaults to its existing slim mode; the full CStrip2 editor remains available by expanding a strip. DSP and existing project timing are unchanged.
- Opening a bottom panel gives it usable content height. Rack parameter columns adjust to available height rather than reserving hidden rows. Tabs fit their labels; buttons fit labels to their bounds. Missing ellipsis and divider glyphs now render.

These changes do not implement the proposed Ableton-style device redesign, stereo asset store, tempo-map engine, real recording, safe stretch or the reproduced clip-gesture fixes. Full-strip EQ/comp display, device-specific visualizers and other screens still require broader layout review; the follow-up rules require concrete defects to be addressed as encountered rather than declaring the entire UI fixed.


Mox guidance was adapted from pyrotek45/mox-tracker at `ed7eb0456f711281cccb1c74a0869b675b0effae`. Eden's AGENTS.md includes the useful general engineering, owner-merge, audio-quality, DSP-research, units/provenance, migration and verification rules. It excludes the eight-input product contract and manual controller checks, replacing them with desktop pointer/keyboard guidance.

Validation: full suite **564 passed, 0 failed**, followed by the two targeted toolbar/layout regressions passing after final layout refinements. Build succeeds and `git diff --check` is clean. Actual screenshots were reviewed at 1280×800 and 800×700: [compact mixer](screenshots/after-mixer.png), [small-window toolbar/mixer](screenshots/after-small.png), [small-window rack](screenshots/after-rack-small.png). Larger font action-layout bounds are covered at font scales 1–4 by the layout regression; that is not a claim that every other view was visually checked at every scale. These results describe local validation; GitHub Actions results are reported separately in the pull request.
