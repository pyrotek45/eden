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
