# Rack device displays

Each rack device card can show a display to the right of its knobs. `src/views/rack_visuals.rs` draws them for the track rack and the master rack, so a display is changed in one place. The display column is `DEVICE_VISUAL_COLUMN_WIDTH_PX` wide (220 px); the old 120 px strip was too small to read as a graph.

A display must describe what the DSP does, not an idealised version of it. Analytic responses live in `src/modules/response.rs`, and each one is tested against the real effect:

| Device | Display | Source of truth | Test |
|---|---|---|---|
| LP Filter, HP Filter | Log-frequency magnitude response, decade labels, cutoff marker and value, Output gain applied | `svf_magnitude_db`, derived from `svf_tick` | Real filters driven with sine tones at 15 cutoff/resonance/frequency combinations; agreement within 0.5 dB. Breaking the damping term makes the tests fail. |
| Reverb | Time axis with pre-delay marker, early-reflection stems at their tap times, late tail falling 60 dB over the Decay time | `reverb_early_taps`, built from constants shared with `FxReverb::process_early` | Impulse through the real reverb: a reflection must appear at every displayed tap time. Shifting the displayed times makes the test fail. |
| Compressor, EQ, Distortion, Delay, Limiter | Unchanged from before, moved into the shared module | Existing curves | Not yet covered by response tests |

The filter graph previously read a parameter named `resonance` (the real id is `reso`) and used a different Q model, so it never followed the Resonance knob. That is fixed.

## Not covered

- The reverb display shows the mid-band tail only. The measured slope matches the Decay knob within about 6-15% (damping and diffusion remove a little more). It does not draw the High/Low Mult bands: those scale per-loop feedback, not RT60, and the relationship has not been verified.
- **Reverb Low Mult is unstable above about 1.1** (control range 0.5-2.5). With Decay 2.0 s, Low Mult 1.2 grows from 5.9e-2 at 5 s to 7.8e2 at 19 s; 1.4 reaches 1e33; 2.5 reaches `inf` within five seconds. Cause: the low-band comb feedback is `fb_base * low_mult` and only `fb_base` is clamped below 1 (`reverb.rs`, `process_late`). This is a DSP safety defect, not a display problem, and is not changed here. Acceptance for a fix: the tail decays for every control value, output stays finite, and a test sweeps the range. Per the repository rules the fix needs its own DSP review.
- No display yet for Chorus, Gain, Utility, Autoduck or CStrip2; the instruments have none.
- Displays are not yet interactive. The draggable handles (cutoff on the curve, threshold on the compressor) are the next step and must edit the same parameters as the knobs.
- Reference points: Ableton's device views pair a live response display with a compact knob group. This pass was designed from that general description, not from Ableton's documentation, which was not consulted.

## Validation

- `cargo test --locked`: all unit and integration tests pass, including the new response and display tests.
- Real X11/SDL runs at 1280x800 with the SDL dummy audio driver: LP Filter cutoff and Resonance knobs were dragged with the mouse and the curve, cutoff marker and value followed; Reverb displays were captured at Decay 3.0 s. Compressor, EQ, Distortion, Delay and Limiter displays were not re-captured after the move; they use the same code, but that is unverified visually.
- No listening or latency checks were possible.
