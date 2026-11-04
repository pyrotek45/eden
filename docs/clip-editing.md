# Clip editing: current repair and ownership migration

## Implemented interaction contract

- Click the clip body or header to select. Drag either to move the selection.
- Shift-click toggles membership, including at clip edges. It does not stretch.
- Ctrl held when a drag starts copies the selection; originals stay in place. A click alone creates no copy. Alt is not an undocumented copy shortcut.
- Edge drags trim. Short clips retain distinct edge handles. Selecting an unselected edge makes that clip the active editor target.
- Double-click the body to open the adaptive editor; double-click the name/header to rename. Modified clicks do not start rename.
- Snap applies to the edited boundary's arrangement beat: start for moves/left trim, end for right trim. With snap enabled, the boundary snaps to the selected grid; with snap disabled, it moves freely. A pointer-down/release with no drag never quantizes an off-grid clip.
- Group operations use one constrained delta. Beat-zero and audio source limits cannot squash spacing or independently shorten different members. A cross-track move/copy is accepted only if every destination exists and matches its source track type.
- Preview derives from immutable pointer-down metadata. Release uses the final pointer position and commits one undo snapshot. Escape or window focus loss restores the original metadata without adding history or marking an unchanged project dirty.
- Ctrl+D duplicates the entire selection by its total span and points the editor at the new copies. Normal duplicates are independent variations for every type while immutable audio assets remain reused. Explicit linked duplication is described in [clip-content.md](clip-content.md).
- Switching clips clears stale note/point/sample selections, retaining editor zoom. Editors continue to read the project clip directly, rather than an independent editor copy.

Implementation: `src/app/clip_edit.rs`. A gesture stores only start, length, audio offset and captured source bounds for selected clips, not cloned note/sample buffers or a full project per pointer frame. The existing history system still stores a full project once on commit; replacing that system is separate work.

WAV source bounds come from headers and Ogg/Vorbis bounds from final granule metadata, even before waveform generation. Bounds are captured once, so a cache becoming ready cannot change the rules in the middle of a drag. Missing/unreadable or unsupported container metadata can still fall back to available waveform metadata.

The old “TIME STRETCH” decoration was removed: the underlying operation was only a length change. True stretch remains deferred. No DSP/stretch algorithm changes are included.

## DAW interaction references

Reviewed Ardour's official manual source (accessed 2026-10-19):

- [Moving regions with the mouse](https://github.com/Ardour/manual/blob/master/include/move-regions-with-the-mouse.html): body dragging, moving a selection as a group.
- [Copying regions](https://github.com/Ardour/manual/blob/master/include/copy-regions.html): modifier-drag copies and grouped movement.
- [Trimming regions](https://github.com/Ardour/manual/blob/master/include/trimming-regions.html): edge drag and shared snap settings.

These support the interaction structure, not a claim that Eden should copy Ardour's exact modifier map. Eden keeps the user's Shift-click selection and Ctrl-drag copy conventions. Ableton's manual was inaccessible under the cloud network policy; it was not used as verified evidence.

## Content ownership

The ownership migration is implemented in [clip-content.md](clip-content.md),
including stable identities, linked/unique content, source-preserving audio assets,
shared history, source offsets, shelf persistence and memory behavior.

The obsolete full Edit/Mixer page renderers were removed. The live workflow uses
Arrangement with bottom Mixer/Edit/Rack panels, so the old full-page audio renderer
cannot cover its clip sidebar.

## Visual follow-up recorded during validation

The audio-toolbar overflow observed at 800×700 is repaired by the measured wrapping layout in [ui-controls.md](ui-controls.md), including UNIQUE and dropdown overlay positions. The full Edit-page sidebar issue remains separate. Audio/project history is still split: FIT stores a project snapshot, while focused audio Undo reads the file-history stack. A separate history pass must make interleaved file/clip-parameter edits undo in the user's actual edit order.

## Validation of this patch

- `cargo test --locked`: 579 unit tests and 2 SDL integration tests passed, including 15 new gesture/editor/hit-target regressions.
- `cargo build --locked` and `cargo rustc --locked --bin eden -- -D warnings` passed on Rust 1.90.0.
- Real X11/SDL input with a synthetic two-MIDI-clip/audio fixture verified body moves, Shift group selection, group beat-zero clamping, group right-trim undo, Escape cancellation, invalid Ctrl-copy destinations, and repeated Ctrl+D. Each result was checked by saving and inspecting project JSON.
- Audio left/right edge input verified source offset, duration and undo; the source WAV hash remained unchanged.
- Captures reviewed at 1440×1000 and 800×700. At 150% UI scale, a real body move and undo produced the same saved beat positions. Help content scrolling was verified at a 960×600 logical viewport.
- Audio used an ALSA null sink. These are editing, rendering and numerical checks, not listening, device-latency or physical MIDI validation.

Screenshots and local UI-driving scripts/logs are outside tracked source in the cloud workspace's `eden-clip-validation` directory. The shared-content migration and audio-editor layout defects above are not claimed fixed by these checks.
