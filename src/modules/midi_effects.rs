// Eden DAW — MIDI Effects
//
// Chainable MIDI processing modules: Transpose, Velocity, Chord, Arpeggiator.

use super::{MidiContext, MidiEffect, MidiEvent, MidiEvents};

// ── Transpose ────────────────────────────────────────────────────────

pub struct MfxTranspose;

impl MidiEffect for MfxTranspose {
    fn name(&self) -> &'static str {
        "Transpose"
    }
    fn process(&mut self, events: MidiEvents, ctx: &MidiContext<'_>) -> MidiEvents {
        let semitones = ctx.get("semitones") as i32;
        let octave = ctx.get("octave") as i32;
        let shift = semitones + octave * 12;
        events
            .into_iter()
            .map(|mut e| {
                e.pitch = (e.pitch as i32 + shift).clamp(0, 127) as u8;
                e
            })
            .collect()
    }
    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxTranspose)
    }
}

// ── Velocity ─────────────────────────────────────────────────────────

pub struct MfxVelocity;

impl MidiEffect for MfxVelocity {
    fn name(&self) -> &'static str {
        "Velocity"
    }
    fn process(&mut self, events: MidiEvents, ctx: &MidiContext<'_>) -> MidiEvents {
        let amount = ctx.get("amount");
        let curve = ctx.get("curve");
        let min_vel = ctx.get("min_vel") / 127.0;
        let max_vel = ctx.get("max_vel") / 127.0;
        let (min_vel, max_vel) = (min_vel.min(max_vel), min_vel.max(max_vel));
        events
            .into_iter()
            .map(|mut e| {
                let curved = if curve > 0.5 {
                    e.velocity.powf(1.0 / (curve * 2.0).max(0.01))
                } else {
                    e.velocity.powf((1.0 - curve) * 2.0)
                };
                e.velocity = (curved + amount).clamp(min_vel, max_vel);
                e
            })
            .collect()
    }
    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxVelocity)
    }
}

// ── Chord ─────────────────────────────────────────────────────────────

pub struct MfxChord;

impl MidiEffect for MfxChord {
    fn name(&self) -> &'static str {
        "Chord"
    }
    fn process(&mut self, events: MidiEvents, ctx: &MidiContext<'_>) -> MidiEvents {
        let chord_type = ctx.get("type") as i32;
        let voicing = ctx.get("voicing") as i32;
        let intervals: &[i32] = match chord_type {
            0 => &[4i32, 7],
            1 => &[3i32, 7],
            2 => &[4i32, 7, 10],
            3 => &[3i32, 7, 10],
            4 => &[5i32, 7],
            5 => &[3i32, 6],
            _ => &[4i32, 7],
        };
        let mut out = MidiEvents::new();
        for e in events.iter() {
            out.push(e.clone()); // keep root
            for (idx, &interval) in intervals.iter().enumerate() {
                let octave_shift = match voicing {
                    0 => 0,
                    1 => {
                        if idx % 2 == 1 {
                            12
                        } else {
                            0
                        }
                    }
                    2 => (idx as i32) * 12,
                    _ => 0,
                };
                let new_pitch = (e.pitch as i32 + interval + octave_shift).clamp(0, 127) as u8;
                out.push(MidiEvent {
                    pitch: new_pitch,
                    velocity: e.velocity,
                    original_pitch: e.original_pitch,
                });
            }
        }
        out
    }
    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxChord)
    }
}

// ── Arpeggiator ───────────────────────────────────────────────────────

/// Each rack device owns an arpeggiator instance, independent of individual notes.
pub struct MfxArpeggiator {
    pub step: usize,
    pub last_beat: f64,
    gate_end: f64,
    held_input: MidiEvents,
}

impl MfxArpeggiator {
    pub fn new() -> Self {
        Self {
            step: 0,
            last_beat: -999.0,
            gate_end: -999.0,
            held_input: MidiEvents::new(),
        }
    }
}

impl MidiEffect for MfxArpeggiator {
    fn name(&self) -> &'static str {
        "Arpeggiator"
    }

    fn process(&mut self, events: MidiEvents, ctx: &MidiContext<'_>) -> MidiEvents {
        if events.is_empty() && !ctx.upstream_clocked {
            self.reset();
            return MidiEvents::new();
        }
        if ctx.upstream_clocked && !events.is_empty() {
            self.held_input = events.clone();
        }
        let rate_beats = ctx.get("rate").max(0.0625) as f64;
        let fire = self.last_beat < 0.0
            || (ctx.pos_beats / rate_beats).floor() > (self.last_beat / rate_beats).floor();
        if !fire {
            return MidiEvents::new();
        }
        let events = if events.is_empty() {
            self.held_input.clone()
        } else {
            events
        };
        if events.is_empty() {
            return MidiEvents::new();
        }
        let octaves = ctx.get("octaves").max(1.0) as i32;
        let pattern = ctx.get("pattern") as i32;
        let vel = ctx.get("vel").clamp(0.0, 1.0);

        // Build note pool (sorted ascending pitches × octaves)
        let mut pool = MidiEvents::new();
        let mut sorted = events;
        sorted.as_mut_slice().sort_unstable_by_key(|e| e.pitch);
        let mut previous = None;
        let mut pitches = MidiEvents::new();
        for event in sorted {
            if previous != Some(event.pitch) {
                pitches.push(event);
                previous = Some(event.pitch);
            }
        }
        for oct in 0..octaves.clamp(1, 4) {
            for event in pitches.iter() {
                let mut event = *event;
                event.pitch = (event.pitch as i32 + oct * 12).clamp(0, 127) as u8;
                if vel > 0.0 {
                    event.velocity = vel;
                }
                pool.push(event);
            }
        }

        // Apply pattern ordering
        match pattern {
            1 => pool.as_mut_slice().reverse(),
            2 => {
                let count = pool.len();
                for i in (1..count.saturating_sub(1)).rev() {
                    let event = pool.iter().nth(i).copied().unwrap();
                    pool.push(event);
                }
            }
            3 => {
                let seed = (ctx.pos_beats * 1000.0) as u64;
                for i in (1..pool.len()).rev() {
                    let j = seed
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407)
                        .wrapping_shr(33) as usize
                        % (i + 1);
                    pool.as_mut_slice().swap(i, j);
                }
            }
            _ => {} // 0 = up (default, already sorted)
        }

        let idx = self.step % pool.len();
        let event = pool.iter().nth(idx).copied().unwrap();
        self.step = (self.step + 1) % pool.len();
        self.last_beat = ctx.pos_beats;
        self.gate_end = ctx.pos_beats + rate_beats * ctx.get("gate_len") as f64;
        std::iter::once(event).collect()
    }

    fn reset(&mut self) {
        self.step = 0;
        self.last_beat = -999.0;
        self.gate_end = -999.0;
        self.held_input.clear();
    }

    fn gate_open(&self, pos_beats: f64) -> bool {
        pos_beats < self.gate_end
    }

    fn manages_voices(&self) -> bool {
        true
    }

    fn fresh(&self) -> Box<dyn MidiEffect> {
        Box::new(MfxArpeggiator::new())
    }
}

/// Instantiate a MIDI effect by name.  Returns `None` for unknown names.
pub fn create_midi_effect(name: &str) -> Option<Box<dyn MidiEffect>> {
    match name {
        "Transpose" => Some(Box::new(MfxTranspose)),
        "Velocity" => Some(Box::new(MfxVelocity)),
        "Chord" => Some(Box::new(MfxChord)),
        "Arpeggiator" => Some(Box::new(MfxArpeggiator::new())),
        _ => None,
    }
}

pub(crate) static ARPEGGIATOR_PARAMS: &[super::ParamDesc] = &[
    super::ParamDesc {
        id: "rate",
        name: "Rate (beats)",
        default: 0.25,
        min: 0.0625,
        max: 2.0,
        options: None,
    },
    super::ParamDesc {
        id: "octaves",
        name: "Octaves",
        default: 1.0,
        min: 1.0,
        max: 4.0,
        options: None,
    },
    super::ParamDesc {
        id: "pattern",
        name: "Pattern",
        default: 0.0,
        min: 0.0,
        max: 3.0,
        options: Some(&["Up", "Down", "UpDown", "Random"]),
    },
    super::ParamDesc {
        id: "gate_len",
        name: "Gate",
        default: 0.8,
        min: 0.1,
        max: 1.0,
        options: None,
    },
];
pub(crate) static CHORD_PARAMS: &[super::ParamDesc] = &[
    super::ParamDesc {
        id: "type",
        name: "Type",
        default: 0.0,
        min: 0.0,
        max: 5.0,
        options: Some(&["Major", "Minor", "7th", "Min7", "Sus4", "Dim"]),
    },
    super::ParamDesc {
        id: "voicing",
        name: "Voicing",
        default: 0.0,
        min: 0.0,
        max: 2.0,
        options: Some(&["Close", "Open", "Spread"]),
    },
];
pub(crate) static TRANSPOSE_PARAMS: &[super::ParamDesc] = &[
    super::ParamDesc {
        id: "semitones",
        name: "Semitones",
        default: 0.0,
        min: -24.0,
        max: 24.0,
        options: None,
    },
    super::ParamDesc {
        id: "octave",
        name: "Octave",
        default: 0.0,
        min: -3.0,
        max: 3.0,
        options: None,
    },
];
pub(crate) static VELOCITY_PARAMS: &[super::ParamDesc] = &[
    super::ParamDesc {
        id: "amount",
        name: "Amount",
        default: 0.0,
        min: -1.0,
        max: 1.0,
        options: None,
    },
    super::ParamDesc {
        id: "curve",
        name: "Curve",
        default: 0.5,
        min: 0.0,
        max: 1.0,
        options: None,
    },
    super::ParamDesc {
        id: "min_vel",
        name: "Min Vel",
        default: 1.0,
        min: 0.0,
        max: 127.0,
        options: None,
    },
    super::ParamDesc {
        id: "max_vel",
        name: "Max Vel",
        default: 127.0,
        min: 0.0,
        max: 127.0,
        options: None,
    },
];
