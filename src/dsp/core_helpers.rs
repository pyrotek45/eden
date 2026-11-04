// Eden DAW — DSP core helpers
//
// Constants, DC-HP filter, MIDI helpers, panning, parameter smoothing.

use crate::modules::{MidiContext, MidiEffect, MidiEvents};

// ── Constants ────────────────────────────────────────────────────────────────

/// DC-offset one-pole HP filter coefficient (fc ≈ 20 Hz)
pub const DC_HP_R: f64 = 0.99972;

/// Equal-power micro-fade length at clip boundaries (~5 ms = 220 samples at 44.1kHz)
pub(crate) const CLIP_FADE_SAMPLES: usize = 220;

// ── MIDI effect chain helper ─────────────────────────────────────────────────

/// Run `events` through every MIDI effect instance in `chain`, using the
/// matching param slices from `param_slices`.  Returns the final event list.
pub fn run_midi_chain<'a>(
    mut events: MidiEvents,
    chain: &'a mut [Box<dyn MidiEffect>],
    param_slices: impl Iterator<Item = &'a Vec<(String, f32)>>,
    pos_beats: f64,
    prev_beats: f64,
    bpm: f64,
    sample_rate: f64,
) -> MidiEvents {
    if events.is_empty() {
        for fx in chain.iter_mut() {
            fx.reset();
        }
        return events;
    }
    let mut upstream_clocked = false;
    for (fx, params) in chain.iter_mut().zip(param_slices) {
        let ctx = MidiContext {
            module_name: fx.name(),
            upstream_clocked,
            pos_beats,
            prev_beats,
            bpm,
            sample_rate,
            params: params.as_slice(),
        };
        events = fx.process(events, &ctx);
        if fx.manages_voices() {
            upstream_clocked = fx.gate_open(pos_beats);
        }
    }
    events
}

// ── MIDI → frequency ─────────────────────────────────────────────────────────

#[inline]
pub fn midi_to_freq(pitch: u8) -> f64 {
    440.0 * crate::modules::fast_pow2((pitch as f64 - 69.0) / 12.0)
}

// ── DC HP filter state ───────────────────────────────────────────────────────

/// State for the DC-offset one-pole high-pass filter (stereo).
pub struct DcHpState {
    pub x_l: f64,
    pub y_l: f64,
    pub x_r: f64,
    pub y_r: f64,
}

impl DcHpState {
    pub fn new() -> Self {
        Self {
            x_l: 0.0,
            y_l: 0.0,
            x_r: 0.0,
            y_r: 0.0,
        }
    }

    /// Apply one-pole HP filter (fc ≈ 20 Hz). Removes slowly-drifting DC bias.
    #[inline]
    pub fn process(&mut self, l: f64, r: f64) -> (f64, f64) {
        let new_l = l - self.x_l + DC_HP_R * self.y_l;
        self.x_l = l;
        self.y_l = new_l;
        let new_r = r - self.x_r + DC_HP_R * self.y_r;
        self.x_r = r;
        self.y_r = new_r;
        (new_l, new_r)
    }
}

// ── Panning ──────────────────────────────────────────────────────────────────

/// Apply equal-power panning and volume to a track's signal and add to stereo mix.
/// Returns the contribution (left, right) for this track.
#[inline]
pub fn pan_and_mix(signal: (f64, f64), pan: f64, volume: f64) -> (f64, f64) {
    let theta = (pan + 1.0) * 0.5 * std::f64::consts::FRAC_PI_2;
    let pan_l = crate::modules::fast_cos(theta) * std::f64::consts::SQRT_2;
    let pan_r = crate::modules::fast_sin(theta) * std::f64::consts::SQRT_2;
    (signal.0 * pan_l * volume, signal.1 * pan_r * volume)
}

// ── Parameter smoothing ──────────────────────────────────────────────────────

/// Smooth a flat parameter list toward its target values using an
/// exponential moving average.  New parameters are initialised to their
/// target value (no ramp from zero on first appearance).
///
/// Used for per-track effect params, master rack params, and CStrip2
/// channel-strip params.
#[inline]
pub fn smooth_params(cache: &mut Vec<(String, f32)>, target: &[(String, f32)], coeff: f32) {
    // Grow the cache if new params appeared
    if cache.len() < target.len() {
        for item in target.iter().skip(cache.len()) {
            cache.push(item.clone());
        }
    }
    // Smooth each value toward target
    for (pi, p) in target.iter().enumerate() {
        if pi < cache.len() {
            cache[pi].1 += (p.1 - cache[pi].1) * coeff;
        }
    }
}

#[cfg(test)]
mod midi_chain_tests {
    use super::*;
    use crate::app::models::create_rack_slot_for_module;
    use crate::modules::{create_midi_effect, MidiEvent};

    fn chain(names: &[&str], beat: f64) -> Vec<(u8, f32, u8)> {
        let mut modules: Vec<_> = names
            .iter()
            .map(|n| create_midi_effect(n).unwrap())
            .collect();
        let slots: Vec<_> = names
            .iter()
            .map(|n| {
                let mut slot = create_rack_slot_for_module(n, 1);
                for param in &mut slot.params {
                    if param.id == "semitones" {
                        param.value = 12.0;
                    }
                    if param.id == "amount" {
                        param.value = -0.25;
                    }
                }
                slot.params
                    .into_iter()
                    .map(|p| (p.id, p.value))
                    .collect::<Vec<_>>()
            })
            .collect();
        run_midi_chain(
            std::iter::once(MidiEvent::new(60, 0.75)).collect(),
            &mut modules,
            slots.iter(),
            beat,
            beat - 0.001,
            120.0,
            44100.0,
        )
        .into_iter()
        .map(|e| (e.pitch, e.velocity, e.original_pitch))
        .collect()
    }
    #[test]
    fn arp_composes_with_effects_in_rack_order_and_keeps_source_velocity() {
        for names in [
            ["Transpose", "Arpeggiator", "Velocity"],
            ["Arpeggiator", "Transpose", "Velocity"],
        ] {
            assert_eq!(chain(&names, 0.0), vec![(72, 0.5, 60)]);
        }
        assert_eq!(
            chain(&["Arpeggiator", "Chord"], 0.0)
                .iter()
                .map(|e| e.0)
                .collect::<Vec<_>>(),
            vec![60, 64, 67]
        );
        assert_eq!(chain(&["Chord", "Arpeggiator"], 0.0).len(), 1);
    }
    #[test]
    fn timed_gate_closes_and_empty_input_resets_next_step() {
        let mut fx = create_midi_effect("Arpeggiator").unwrap();
        let slot = create_rack_slot_for_module("Arpeggiator", 1);
        let params: Vec<_> = slot.params.into_iter().map(|p| (p.id, p.value)).collect();
        let context = |beat| crate::modules::MidiContext {
            module_name: "Arpeggiator",
            upstream_clocked: false,
            pos_beats: beat,
            prev_beats: beat - 0.001,
            bpm: 120.0,
            sample_rate: 44100.0,
            params: &params,
        };
        assert_eq!(
            fx.process(
                std::iter::once(MidiEvent::new(60, 0.7)).collect(),
                &context(0.0)
            )
            .len(),
            1
        );
        assert!(fx.gate_open(0.1));
        assert!(!fx.gate_open(0.21));
        assert!(fx.process(MidiEvents::new(), &context(0.22)).is_empty());
        assert_eq!(
            fx.process(
                std::iter::once(MidiEvent::new(60, 0.7)).collect(),
                &context(0.23)
            )
            .len(),
            1
        );
    }
    #[test]
    fn omitted_parameters_use_device_defaults_and_crossed_velocity_bounds_are_safe() {
        let mut chain: Vec<_> = ["Transpose", "Arpeggiator", "Velocity"]
            .iter()
            .map(|name| create_midi_effect(name).unwrap())
            .collect();
        let parameters = vec![Vec::new(), Vec::new(), Vec::new()];
        let output = run_midi_chain(
            std::iter::once(MidiEvent::new(60, 0.75)).collect(),
            &mut chain,
            parameters.iter(),
            0.0,
            -0.01,
            120.0,
            44100.0,
        );
        let event = output.iter().next().unwrap();
        assert_eq!(event.pitch, 60);
        assert_eq!(event.velocity, 0.75);
        let mut fx = create_midi_effect("Velocity").unwrap();
        let params = vec![("min_vel".into(), 127.0), ("max_vel".into(), 1.0)];
        let context = crate::modules::MidiContext {
            module_name: "Velocity",
            upstream_clocked: false,
            pos_beats: 0.0,
            prev_beats: 0.0,
            bpm: 120.0,
            sample_rate: 44100.0,
            params: &params,
        };
        let result = fx.process(
            std::iter::once(MidiEvent::new(60, 0.75)).collect(),
            &context,
        );
        assert_eq!(result.iter().next().unwrap().velocity, 0.75);
    }
    #[test]
    fn serial_arpeggiators_keep_the_downstream_clock_between_input_events() {
        let mut effects: Vec<_> = (0..2)
            .map(|_| create_midi_effect("Arpeggiator").unwrap())
            .collect();
        let params = vec![
            vec![("rate".into(), 0.25), ("gate_len".into(), 1.0)],
            vec![("rate".into(), 0.5), ("gate_len".into(), 1.0)],
        ];
        for (beat, expected) in [(0.0, 1), (0.01, 0), (0.25, 0), (0.3, 0), (0.5, 1)] {
            let out = run_midi_chain(
                std::iter::once(MidiEvent::new(60, 0.75)).collect(),
                &mut effects,
                params.iter(),
                beat,
                beat - 0.001,
                120.0,
                44100.0,
            );
            assert_eq!(out.len(), expected, "beat {beat}");
        }
    }
}
