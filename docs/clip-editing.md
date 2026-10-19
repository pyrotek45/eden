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
- Ctrl+D duplicates the entire selection by its total span and points the editor at the new copies. MIDI/automation retain their existing independent-copy behavior; audio continues sharing its source file. This patch does not introduce linked MIDI/automation.
- Switching clips clears stale note/point/sample selections, retaining editor zoom. Editors continue to read the project clip directly, rather than an independent editor copy.

Implementation: `src/app/clip_edit.rs`. A gesture stores only start, length, audio offset and captured source bounds for selected clips, not cloned note/sample buffers or a full project per pointer frame. The existing history system still stores a full project once on commit; replacing that system is separate work.

WAV source bounds can be read from the header at gesture start even before waveform generation. Other formats currently use available waveform metadata. Bounds are captured once, so a cache becoming ready cannot change the rules in the middle of a drag. Missing/unreadable source metadata remains a limitation, particularly for cold OGG clips.

The old “TIME STRETCH” decoration was removed: the underlying operation was only a length change. True stretch remains deferred. No DSP/stretch algorithm changes are included.

## DAW interaction references

Reviewed Ardour's official manual source (accessed 2026-10-19):

- [Moving regions with the mouse](https://github.com/Ardour/manual/blob/master/include/move-regions-with-the-mouse.html): body dragging, moving a selection as a group.
- [Copying regions](https://github.com/Ardour/manual/blob/master/include/copy-regions.html): modifier-drag copies and grouped movement.
- [Trimming regions](https://github.com/Ardour/manual/blob/master/include/trimming-regions.html): edge drag and shared snap settings.

These support the interaction structure, not a claim that Eden should copy Ardour's exact modifier map. Eden keeps the user's Shift-click selection and Ctrl-drag copy conventions. Ableton's manual was inaccessible under the cloud network policy; it was not used as verified evidence.

## Ownership findings: still unresolved

1. `MidiClip.notes` and `AutomationClip.points` are owned vectors. Cloning copies their content, so there is no persistent link for shared edits.
2. `AudioClip.source_file` is a path. Multiple clips referencing it share file edits, but this conflates immutable source storage with editable clip content.
3. The audio editor writes transformations into source files. UNIQUE copies beside the original source, which duplicates the entire asset, can fail in read-only folders, and is not a general shared/unique clip model. Undo uses separate file backups and a separate history path.
4. `sync_clip_library` adds copies matched by track/name/type and does not refresh existing content. Same-named variations can collapse into one browser entry; edits can leave stale browser payloads. Browser entries are not authoritative shared content.
5. Arrangement/editor selections use `(track_id, vector_index)`. Removing, sorting or moving clips can change identity. Clearing invalid indices helps, but does not solve indices that remain valid while referring to a different clip.
6. Full project snapshots and playback-state clones copy MIDI/automation vectors. Decoded audio is cached in multiple forms with no explicit budget. A shared-content migration must cover publication, history and caches together.
7. MIDI/automation left trim currently lacks an explicit content offset. A complete non-destructive trim model must preserve the song time of retained events and expose previously hidden content on expansion. The gesture repair alone does not establish that model.
8. The full Edit page's audio/automation renderer uses full-window coordinates and can cover its clip sidebar. This needs the same offset/scissor contract used by the piano roll; acceptance: sidebar remains visible and clickable with all three clip types at small/default sizes.

## Target model: instances, content and assets

Use three stable identities:

| Identity | Owns | Sharing behavior |
|---|---|---|
| ClipId | Arrangement placement, visible boundaries and content offset | Moving/trimming one instance does not move/trim other instances |
| ContentId | MIDI notes, automation points/target, or non-destructive audio edit description | Editing linked content updates every instance using that ContentId |
| AssetId | Immutable original media, channel/sample-rate metadata | Many independent contents may reuse the same decoded/source storage |

A linked duplicate gets a new ClipId and the same ContentId. **Make Unique** creates a new ContentId for that instance, copying the content description while retaining immutable AssetIds. Audio data is copied/rendered only when a transformation actually requires a derived asset, not to make a variation. Display a visible linked-count/status and expose explicit Linked Duplicate and Make Unique actions. Preserve normal duplicate semantics until a separately reviewed change specifies otherwise.

The library, arranger, editor and engine must resolve the same ContentId and revision. Revision changes invalidate waveform/derived caches and publish prepared playback state atomically. Separate editor zoom/scroll/selection by view or content identity; never use the displayed name as identity. Linked data synchronization must include undo/redo, deletion, loading and offline export, not only visible editors.

## Next migration, in reviewable steps

1. Add stable identities and a versioned loader. Assign identities to legacy clips without inferring MIDI links from equal names/notes. Preserve legacy audio file sharing explicitly so loading cannot change sound.
2. Store authoritative content in the project and make all editors/commands resolve it. Add source/content offsets for non-destructive trim. Remove browser-owned mutable copies and name/type matching after callers migrate.
3. Implement explicit linked duplication and Make Unique for all three types. Tests: edit A updates linked B; unique C diverges; moving/trimming A only affects A; undo/redo and save/reopen preserve links and isolation.
4. Move audio edits to immutable sources plus edit descriptions or versioned derived assets in project-managed storage. Unwritable imported samples must remain editable without modifying originals. Reconcile audio and project history in one transaction model.
5. Bound decoded/derived caches by bytes, share prepared assets, and keep loading, allocation and reclamation away from the audio callback. Verify stereo, cache eviction, realtime/export consistency and compatibility fixtures.

Acceptance includes switching between every editor while editing linked and unique clips; equal display names must never merge unrelated data. Measure heap use for repeated clips and audio buffers before/after. Preserve original files and provide project recovery throughout the migration.

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
