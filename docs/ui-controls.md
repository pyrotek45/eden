# Theme colors and audio-editor control layout

## SDL2_gfx boundary

rust-sdl2 0.38 packs `Color` into a big-endian integer for its SDL2_gfx calls. The native SDL2_gfx `*Color` APIs interpret the integer's memory as RGBA bytes. On little-endian machines this reverses red/alpha and green/blue, so neutral knob bodies become reddish and partly transparent. The renderer regression reproduced a requested `[50, 50, 50]` as `[50, 9, 9]`.

`src/widgets/gfx.rs` passes native-order RGBA bytes explicitly and preserves the canvas blend mode/drawing color around native primitives. SDL2_gfx changes those states internally; leaving blending disabled makes later translucent popups opaque black. Knob arc segments share one state-preserving batch to avoid querying/resetting SDL for every segment. The main canvas explicitly enables alpha blending at startup rather than relying on a native primitive's side effect. Normal SDL Canvas operations continue receiving ordinary Color values.

All knob, meter, compressor-handle and button graphics use this boundary. The theme's knob body/indicator and accent values remain authoritative; no red-to-orange substitution or palette override is involved. Rack selectors and dropdown text/backgrounds also use theme roles so light themes remain readable.

Native contract: [SDL2_gfxPrimitives.c](https://github.com/ferzkopp/SDL2_gfx/blob/master/SDL2_gfxPrimitives.c), including `filledCircleColor` and the other packed-color APIs.

## Audio toolbar

`src/widgets/audio_toolbar.rs` measures and wraps whole controls rather than pinning FX over the last tools. Transport, UNIQUE, edit tools, snap/grid, gain/fades, Export and the FX/APPLY pair all use the same rectangles for drawing and input. Dropdown overlays use those same rectangles. The clip name and source/window duration occupy a separate context row. Rulers and waveform geometry derive from the resulting toolbar height.

Measurements account for font scales S–XL and the optional UNIQUE button. Captions do not overlap adjacent controls; the effect selector fits its longest option, and mini transport buttons use real icons. Existing audio transformations, parameter ranges, project data and source-file behavior are unchanged.

## Validation

- Native software-renderer pixels check neutral/cool/warm/green colors, opaque white, opacity, and preservation of blend mode and draw color across seven graphics primitives.
- Toolbar geometry checks every control at 520/640/800/1280/1440 widths, four font scales, and both shared/unique source states.
- Actual SDL captures review the dark, light and midnight rack themes and the shared-source audio toolbar at default, 800 and 520 widths.
- Wrapped FX/APPLY was exercised on a synthetic WAV; audio Undo restored its original hash. The wrapped Export button opened its dialog.

This is a repair of color/layout behavior, not the larger SSL-inspired mixer or Ableton/Bitwig-inspired rack redesign. The separate audio/project history stacks and the shared-content migration remain documented follow-ups in [clip-editing.md](clip-editing.md).

## Console sections and device cards

The expanded mixer groups the existing CStrip2 parameters into Filter, EQ, and
Dynamics/Trim sections beside the level fader and stereo meters. The oversized
track VU gauge and duplicate response graphs have been removed from this panel;
the master meter remains. Compact channels are still the default. Section
scrollbars keep every parameter reachable in short panels and at larger fonts.
Controls retain their existing parameter IDs, ranges and DSP; these are console-inspired
groups, not an implementation of an SSL circuit or an SSL-branded processor.
Missing channel parameter entries use their real defaults and are created only
when edited. Selected-track relative adjustments also work when those entries
have not yet been saved.

Track and master rack cards share a shaded header and a MIDI FX, Instrument or
Audio FX category. Parameter spacing accounts for font height, captions and
selector options. Labelled knobs keep their value and caption in fixed positions
rather than moving their caption on hover. Device parameters and channel knobs
use stable widget identities. Clicking these project controls restores project
undo routing after an audio-editor interaction.

Validation uses actual SDL captures at 1440x1000 and 800x700, including XL text,
plus geometry checks and mouse-driven parameter/bypass edit, save and undo.
Master statistics retain a compact text size inside the fixed-width meter panel.
No audio processing or project format changes are included.

The Options popup now uses compact form text independent of the workspace font,
and centers within the minimum window. Audio ruler/footer readouts also retain a
compact text size. Light-theme arranger titles use theme text colors. MIDI choice
captions align with knob captions, preventing overlap between mixed control rows.
See [clip-content.md](clip-content.md) for the implemented functional follow-up.

Normal split view preserves an arranger/browser viewport at small sizes; an
explicitly maximized detail panel can still cover it. Large workspace fonts raise
the minimum logical window height so controls and a usable split view fit. The
clip browser scales its row heights, uses theme backgrounds, and truncates long
names with a full-name tooltip instead of overlapping its type/beat readouts.

## Rack visual refinement

Track and master device chains now share a flat card surface: neutral headers,
a narrow theme-accent selection rail, a full-card selected outline, aligned
parameter bays and a divider beside device graphs. Only occupied parameter cells
receive a bay. Wider gaps keep left-to-right signal arrows separate from controls.
Knobs use a neutral flat face, an outer theme-colored value arc and a contrasting
position marker; decorative shadows, center dots and glow rings were removed.
Selector captions use the readable secondary text color rather than disabled text.

The end-of-chain action is labeled `+ DEVICE` and opens the existing module
browser. Both rack scroll ranges include its full width. Device parameters,
header hit regions, bypass/delete actions, drag ordering, undo and DSP are unchanged.
The visual direction takes cues from compact Ableton/Bitwig device chains while
retaining Eden's bitmap typography and custom themes; no third-party UI assets
or implementation were copied.
