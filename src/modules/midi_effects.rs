// Eden DAW — MIDI Effects
//
// Chainable MIDI processing modules: Transpose, Velocity, Chord, Arpeggiator.

use super::{MidiContext, MidiEffect, MidiEvent};

// ── Transpose ────────────────────────────────────────────────────────

pub struct MfxTranspose;

impl MidiEffect for MfxTranspose {
    fn name(&self) -> &'static str {
        "Transpose"
    }
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
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
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
        let amount = ctx.get("amount");
        let curve = ctx.get("curve");
        let min_vel = ctx.get("min_vel") / 127.0;
        let max_vel = ctx.get("max_vel") / 127.0;
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
    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
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
        let mut out = Vec::with_capacity(events.len() * (1 + intervals.len()));
        for e in &events {
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

/// Arpeggiator state.  One instance lives per track (not per note).
pub struct MfxArpeggiator {
    pub step: usize,
    pub last_beat: f64,
}

impl MfxArpeggiator {
    pub fn new() -> Self {
        Self {
            step: 0,
            last_beat: -999.0,
        }
    }
}

impl MidiEffect for MfxArpeggiator {
    fn name(&self) -> &'static str {
        "Arpeggiator"
    }

    fn process(&mut self, events: Vec<MidiEvent>, ctx: &MidiContext<'_>) -> Vec<MidiEvent> {
        if events.is_empty() {
            self.step = 0;
            self.last_beat = -999.0;
            return Vec::new();
        }

        let rate_beats = ctx.get("rate").max(0.0625) as f64;
        let octaves = ctx.get("octaves").max(1.0) as i32;
        let pattern = ctx.get("pattern") as i32;
        let vel = ctx.get("vel").clamp(0.0, 1.0);

        // Build note pool (sorted ascending pitches × octaves)
        let mut pool: Vec<MidiEvent> = Vec::new();
        let mut pitches: Vec<u8> = events.iter().map(|e| e.pitch).collect();
        pitches.sort_unstable();
        pitches.dedup();
        let base_vel = events.first().map(|e| e.velocity).unwrap_or(0.8);
        let final_vel = if vel > 0.0 { vel } else { base_vel };

        for oct in 0..octaves {
            for &p in &pitches {
                let shifted = (p as i32 + oct * 12).clamp(0, 127) as u8;
                pool.push(MidiEvent {
                    pitch: shifted,
                    velocity: final_vel,
                    original_pitch: p,
                });
            }
        }

        // Apply pattern ordering
        match pattern {
            1 => pool.reverse(),
            2 => {
                let mut down = pool.clone();
                down.reverse();
                if down.len() > 1 {
                    pool.extend_from_slice(&down[1..down.len() - 1]);
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
                    pool.swap(i, j);
                }
            }
            _ => {} // 0 = up (default, already sorted)
        }

        // Check if a new step should fire
        let fire = if self.last_beat < 0.0 {
            true
        } else {
            let steps_now = (ctx.pos_beats / rate_beats).floor() as usize;
            let steps_last = (self.last_beat / rate_beats).floor() as usize;
            steps_now > steps_last
        };

        if fire {
            let idx = self.step % pool.len();
            let event = pool[idx].clone();
            self.step = (self.step + 1) % pool.len().max(1);
            self.last_beat = ctx.pos_beats;
            vec![event]
        } else {
            Vec::new()
        }
    }

    fn reset(&mut self) {
        self.step = 0;
        self.last_beat = -999.0;
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
