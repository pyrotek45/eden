# Workflow follow-through validation

Implemented behavior and compatibility are documented in [clip-content.md](clip-content.md).

Actual SDL/X11 checks used a synthetic project containing two MIDI instances,
two stereo audio instances and persisted shelf content. The source WAV was
read-only. Shared note editing, Make Unique, undo/relink, deleting to the shelf,
save/reopen and editing a shelf-linked arrangement instance were exercised.
Normalize created a new asset and left source bytes unchanged. Cutting all audio
then pasting restored every float sample and both channels; two undo steps restored
the original source reference. Rack right-drag reorder, an invalid cross-stage
drop, selector click/drag and fader undo were checked through mouse input.

Renderer checks covered default and XL fonts, Light theme, Options at 800px height,
and the 520×700 minimum XL layout. The detail panel retains arranger/browser space.
Screenshots and generated media were kept outside the source tree. Audio ran into
an ALSA silent sink: these checks do not establish physical listening quality,
latency or hard realtime guarantees.

Regression tests include linked content/identity/save migration, immutable audio
edits and unified undo, source-offset trims, beat automation, ordered MIDI effects
and serial arpeggiators, bounded note buffers, cold OGG metadata, UTF-8 text cursor
boundaries, stereo decoding/interpolation and preview frame looping.

Final local checks: `cargo build --locked`, `cargo test --locked` (621 unit +
1 graphics + 2 SDL event tests; 624 passed), and `cargo rustc --locked -- -D warnings`
all passed. GitHub Actions status is reported separately in the pull request.
