# Clip content and workflow

Eden now separates a clip's arrangement instance from its editable content.
Position, length, source offset, name and color belong to the instance. MIDI
notes, automation points/target, and audio source/gain/fades belong to shared
content. Immutable imported/derived audio files are reused by path.

- **Ctrl+D** and **Ctrl-drag** create independent variations without copying WAV
  data. MIDI/automation buffers use copy-on-write until an edit requires a copy.
- **LINK COPY** in Clips, or **Ctrl+Shift+D**, duplicates the selection with shared
  content. Moving or trimming an instance affects its window only. Editing linked
  content updates the other instances, including shelf entries.
- **UNIQUE**, or **Ctrl+Shift+U**, detaches the active clip's content. It does not
  copy an imported audio file. Already-unique clips have no redundant action.
- The Clips browser marks linked content `[L2]`, `[L3]`, etc. Equal names never
  identify or merge content. Shelf entries are project data, persist across
  save/reopen, and participate in project undo. Deleting an arrangement instance
  retains it on the shelf; removing it through the browser removes that entry.
- Editors read the same project content. Their zoom/scroll/selection remain view
  state. Stable instance IDs keep the active editor on its clip after another
  clip is removed or indices change.

## Audio edits and history

Normalize, trim, cut, paste and selection effects write new WAV assets under
`.eden-assets` beside the saved project (or in the working directory for an
unsaved project). Imports can be read-only: no tool overwrites an imported source
or a historical rendered asset. A linked edit publishes one new asset reference
to the linked group; a unique variation keeps its own reference. Source edits
remap linked windows for removed/inserted source time. Window trimming is still
separate from changing the source's contents.

All three editors and toolbar/keyboard shortcuts use one project history. Undo
restores the content reference and accompanying window metadata together. Gain,
fade and device parameter gestures capture their before-state and commit once.
Historical assets remain on disk for recovery; this change does not automatically
delete unused assets or imported files. Keep `.eden-assets` with the project.
Existing external imports and derived paths still need their referenced files;
this is not a new project-packaging/collect-files feature.

## Timing and gestures

MIDI and automation instance offsets are beats into their source content. A left
trim moves the window into that content without moving/deleting retained events;
expanding restores the hidden region. Right trim changes the window, not note
positions or automation playback speed. The offline automation evaluator now uses
the editor's beat coordinates, matching playback; its old normalized-coordinate
interpretation produced different exports.

Arranger gestures retain Shift-click selection, Ctrl-drag copying, absolute
boundary snapping, group constraints, immutable pointer-down previews, Escape
cancellation and one undo per gesture. Each editor retains its own view grid.
True stretching remains deferred; trimming never claims to stretch audio.

## Storage and compatibility

Project format 2 adds stable instance/content identities and persisted shelf
entries. Old inline clip fields remain readable, and the loader migrates legacy
clips without inferring MIDI/automation links from equal names. Legacy audio
references with equal source/gain/fades become linked; differing settings remain
independent while reusing the source asset. Loading does not change existing gain
or fades. Contradictory saved payloads with one content ID are rejected rather
than silently losing data. Newer unsupported project versions are rejected.

`SharedContent<T>` holds a copy-on-write `Arc` revision. Commands publish revisions
before/after edits; UI publication occurs before engine preparation, after view
input and before saving. Project snapshots share content buffers instead of
copying every note/point vector. `reconcile_clip_content` is a UI/command boundary,
not an audio-callback operation. New mutation entry points must follow that rule.
The browser derives from project/shelf identities, rather than owning stale
name-matched copies. Serialized inline payloads are validated and rebound to shared
storage when loaded; the JSON still repeats linked payloads for compatibility.

Prepared MIDI buffers are cached by content revision and source offset, shared
between instances and callback snapshots. Stale decoded assets and prepared notes
are retired on the UI side, keeping large buffers alive until the callback releases
them. Raw editor data retains one selected source, with a 64 MiB cache limit and
peak rendering for larger sources; mono views reuse one buffer. Active playback
assets remain pinned and memory therefore depends on the current song's media.
The existing callback still allocates snapshot/effect state during rack changes;
this is not a claim of a fully allocation-free engine.

## MIDI routing

Keyboard, arranger playback and offline rendering use ordered MIDI processing:
MIDI effects → instrument → audio effects. Arpeggiator input/output passes through
surrounding Transpose, Chord and Velocity devices, retains source velocities and
original-key note-off identity, and honors Gate. Device parameter defaults and
choice labels come from the same registry. Crossed Velocity bounds remain valid.
MIDI work buffers have a deterministic 512-event bound instead of heap-growing
chord/octave expansion in the callback. The existing synthesis/filter algorithms
are unchanged.

Rack headers show selection and hover feedback. Right-drag reordering completes
on right-button release, validates the MIDI/instrument/audio stages, cancels an
outside drop and records one undo step. Legacy mixed-stage racks are stably
ordered to match the engine's existing category passes; effect order within a
stage remains unchanged. Bypass/remove/parameter widgets use device identities.
Mouse-key arpeggiator auditions use a bounded held-note clock rather than bypassing
MIDI processing or silently disappearing between steps. Computer keys retain their
normal audition velocity.

The audio clipboard now keeps interleaved channels. Stereo cut/paste preserves
left/right pairing; mono/stereo conversion happens only when destination layouts
differ. Eden's existing nearest-neighbor rate conversion is retained, not replaced
with a new resampler. Cutting all samples leaves an empty clip that can receive a
paste; its editor displays zero duration instead of an invented ten-second source.


Mono/stereo media now retains its interleaved channel layout through decoded
caches, arranger playback, export and sample preview. Shared clip instances reuse
one decoded asset. The existing mono sampler requests a cached downmix explicitly;
unsupported surround layouts retain the previous mono downmix policy. Linear
interpolation, boundary fades and pan law are unchanged. Frame/channel layout was
checked against [Hound's WavSpec](https://github.com/ruuda/hound/blob/master/src/lib.rs)
and [CPAL's output example](https://github.com/RustAudio/cpal/blob/master/examples/beep.rs).
