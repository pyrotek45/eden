// Eden DAW — Realtime audio engine
//
// Design goals:
//   • Zero blocking in the audio callback — never call .lock(), only .try_lock()
//   • Sample-accurate position tracking via atomic f64 (as u64 bits)
//   • Sample-accurate loop wraparound — no audible glitch on loop boundary
//   • Clip scheduling: only synthesize sound for active MIDI clips at current position
//   • Pixel-perfect / beat-perfect adjacency: clips are rendered from their exact
//     start_time beat, so two adjacent clips (end == next.start) share zero gap
//
// The shared state flows like this:
//   main thread writes  → SharedAudio (Mutex, only via try_lock, low contention)
//   audio callback reads → a snapshot clone taken once per callback, never held across samples
//   position is written  → back via atomic u64 (f64 bits) so the UI can read it
//     without blocking the audio callback at all

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use crate::modules::{
    create_effect, create_instrument, create_midi_effect, voice_is_done, EffectModule,
    InstrumentModule, MidiContext, MidiEffect, MidiEvent, ModuleExtra, ModuleVoice,
};

// ── MIDI effect chain helper ─────────────────────────────────────────

/// Run `events` through every MIDI effect instance in `chain`, using the
/// matching param slices from `param_slices`.  Returns the final event list.
fn run_midi_chain<'a>(
    mut events: Vec<MidiEvent>,
    chain: &'a mut [Box<dyn MidiEffect>],
    param_slices: impl Iterator<Item = &'a Vec<(String, f32)>>,
    pos_beats: f64,
    prev_beats: f64,
    bpm: f64,
    sample_rate: f64,
) -> Vec<MidiEvent> {
    for (fx, params) in chain.iter_mut().zip(param_slices) {
        let ctx = MidiContext {
            pos_beats,
            prev_beats,
            bpm,
            sample_rate,
            params: params.as_slice(),
        };
        events = fx.process(events, &ctx);
    }
    events
}

// ── Shared state (UI → audio) ─────────────────────────────────────────

/// Data the UI pushes each frame; audio callback reads a snapshot.
#[derive(Debug, Clone)]
pub struct AudioShared {
    pub playing: bool,
    pub position_beats: f64,
    pub bpm: f64,
    pub sample_rate: f64,
    pub master_volume: f32,
    pub loop_enabled: bool,
    pub loop_start: f64,
    pub loop_end: f64,
    /// Per-track data: (is_midi, volume, mute, solo, clips)
    pub tracks: Vec<AudioTrack>,
    // ── Metering data (written by audio callback, read by UI) ──
    /// RMS level per track (0.0–1.0) from last callback block.
    pub track_rms: Vec<f32>,
    /// Pre-effect RMS per track — used by the compressor GR meter visual.
    pub track_rms_pre_effect: Vec<f32>,
    // ── Stereo per-track metering ──
    pub track_rms_l: Vec<f32>,
    pub track_rms_r: Vec<f32>,
    // ── Master stereo metering ──
    pub master_rms_l: f32,
    pub master_rms_r: f32,
    /// Ring buffer of recent output samples for the oscilloscope (512 samples).
    pub oscilloscope: Vec<f32>,
    /// Write head into the oscilloscope ring.
    pub osc_write: usize,
    /// Master bus RMS (post-effect, pre-volume).
    pub master_rms: f32,
    /// Master bus pre-effect RMS.
    pub master_rms_pre: f32,
    /// Gain reduction in dB per effect slot, per track. track_idx → Vec<f32>.
    pub track_effect_gr: Vec<Vec<f32>>,
    /// Gain reduction in dB per master rack effect slot.
    pub master_effect_gr: Vec<f32>,
    /// When true: UI has seeked — audio callback must jump to position_beats
    /// and NOT overwrite it until cleared. Cleared by audio callback after reading.
    pub seek_pending: bool,
    // ── Sample preview ───────────────────────────────────────────────
    /// Mono sample data for preview playback (loaded from WAV/FLAC)
    pub preview_samples: Arc<Vec<f32>>,
    /// Current playback position in the preview buffer (in samples)
    pub preview_pos: usize,
    /// Whether preview is actively playing
    pub preview_playing: bool,
    /// Sample rate of the loaded preview file (for resampling)
    pub preview_sample_rate: u32,
    /// End boundary for preview playback (in output samples, 0 = play to end).
    /// When non-zero, playback stops when preview_pos reaches this value.
    pub preview_end_sample: usize,
    /// Whether preview playback should loop (audio editor loop feature).
    pub preview_loop_enabled: bool,
    /// Start position to loop back to (in output samples). Used when preview_loop_enabled is true.
    pub preview_loop_start: usize,
    // ── MIDI note preview ────────────────────────────────────────────
    /// When set: list of (track_index, pitch, velocity) — audio thread spawns voices
    pub preview_notes: Vec<(usize, u8, u8)>,
    /// When set to true, preview voices are sustained (no auto-release timer).
    /// Used by piano keyboard mode so held keys sustain until released.
    pub preview_sustain: bool,
    /// Pitches to immediately release (key-up from piano keyboard mode).
    pub preview_note_off: Vec<u8>,
    /// Currently-held keyboard pitches per track for the keyboard-mode arp clock.
    /// Each entry is (track_index, sorted list of held MIDI pitches).
    /// Updated every UI frame; audio thread uses this to drive the arp when transport is stopped.
    pub preview_held_pitches: Vec<(usize, Vec<u8>)>,
    /// When true, kill all voices immediately (panic button).
    pub panic: bool,
    /// Master rack effect chain: list of (effect_name, params).
    /// Applied to the stereo mix after all tracks are summed, before master_volume.
    pub master_effects: Vec<(String, Vec<(String, f32)>)>,
}

#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub volume: f32,
    pub pan: f32, // -1.0 (full left) to 1.0 (full right), 0.0 = center
    pub mute: bool,
    pub solo: bool,
    /// If true, this track is an automation track and produces no audio.
    pub is_automation: bool,
    /// MIDI clips with their notes baked in
    pub midi_clips: Vec<AudioMidiClip>,
    /// Audio clips with pre-loaded sample data
    pub audio_clips: Vec<AudioSampleClip>,
    // ── Module-based synth ──
    /// Name of the instrument module (e.g. "Analog", "Sampler").
    /// None = no instrument → MIDI track produces no sound.
    pub instrument_module: Option<String>,
    /// Flattened (param_id, value) for the instrument module.
    pub instrument_params: Vec<(String, f32)>,
    // ── Effect chain ──
    /// Ordered list of enabled effect slots: (effect_name, params).
    pub effect_slots: Vec<(String, Vec<(String, f32)>)>,
    /// Ordered list of enabled MIDI effect slots: (effect_name, params).
    pub midi_effect_slots: Vec<(String, Vec<(String, f32)>)>,
    /// Sidechain source track index per effect slot (parallel to effect_slots).
    /// None = use own signal as key (normal). Some(ti) = use track ti's pre-effect output.
    pub effect_sidechain_track: Vec<Option<usize>>,
    /// Channel strip (CStrip2) params — applied after the effect chain, before pan/vol.
    pub cstrip2_params: Vec<(String, f32)>,
    // ── Extra data for instruments (e.g. sampler buffers) ──
    pub extra: ModuleExtra,
}

/// An audio clip with pre-loaded sample data, ready for playback.
#[derive(Debug, Clone)]
pub struct AudioSampleClip {
    /// Where this clip starts in beats
    pub start_beats: f64,
    /// Length in beats
    pub length_beats: f64,
    /// Gain multiplier
    pub gain: f32,
    /// Sample offset in seconds (for trimmed clips - where in the audio file to start)
    pub offset_secs: f64,
    /// Mono sample data (shared via Arc to avoid cloning millions of samples each frame)
    pub samples: Arc<Vec<f32>>,
    /// Original sample rate
    pub sample_rate: u32,
    /// User-controlled fade-in duration in seconds (0 = no fade)
    pub fade_in: f64,
    /// User-controlled fade-out duration in seconds (0 = no fade)
    pub fade_out: f64,
}

#[derive(Debug, Clone)]
pub struct AudioMidiClip {
    pub start_beats: f64,
    pub length_beats: f64,
    pub notes: Vec<AudioNote>,
}

#[derive(Debug, Clone)]
pub struct AudioNote {
    pub pitch: u8,
    pub velocity: u8,
    pub start_beats: f64, // relative to clip start
    pub length_beats: f64,
}

impl Default for AudioShared {
    fn default() -> Self {
        Self {
            playing: false,
            position_beats: 0.0,
            bpm: 120.0,
            sample_rate: 44100.0,
            master_volume: 0.8,
            loop_enabled: false,
            loop_start: 0.0,
            loop_end: 8.0,
            tracks: Vec::new(),
            track_rms: Vec::new(),
            track_rms_pre_effect: Vec::new(),
            track_rms_l: Vec::new(),
            track_rms_r: Vec::new(),
            master_rms_l: 0.0,
            master_rms_r: 0.0,
            oscilloscope: vec![0.0; 512],
            osc_write: 0,
            master_rms: 0.0,
            master_rms_pre: 0.0,
            track_effect_gr: Vec::new(),
            master_effect_gr: Vec::new(),
            seek_pending: false,
            preview_samples: Arc::new(Vec::new()),
            preview_pos: 0,
            preview_playing: false,
            preview_sample_rate: 44100,
            preview_end_sample: 0,
            preview_loop_enabled: false,
            preview_loop_start: 0,
            preview_notes: Vec::new(),
            preview_sustain: false,
            preview_note_off: Vec::new(),
            preview_held_pitches: Vec::new(),
            panic: false,
            master_effects: Vec::new(),
        }
    }
}

pub type SharedAudio = Arc<Mutex<AudioShared>>;

// ── WAV file loading ──────────────────────────────────────────────────

/// Save mono f32 samples to a WAV file (32-bit float, mono).
/// Used by the audio editor for destructive edits (TRIM, CUT).
pub fn save_wav_mono(
    path: &std::path::Path,
    samples: &[f32],
    sample_rate: u32,
) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("WAV create error: {}", e))?;
    for &s in samples {
        writer
            .write_sample(s)
            .map_err(|e| format!("WAV write error: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize error: {}", e))?;
    Ok(())
}

/// Save interleaved stereo f32 samples to a WAV file (32-bit float, stereo).
/// Used by the audio editor for destructive edits on stereo files.
pub fn save_wav_stereo(
    path: &std::path::Path,
    samples: &[f32],
    sample_rate: u32,
) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer =
        hound::WavWriter::create(path, spec).map_err(|e| format!("WAV create error: {}", e))?;
    for &s in samples {
        writer
            .write_sample(s)
            .map_err(|e| format!("WAV write error: {}", e))?;
    }
    writer
        .finalize()
        .map_err(|e| format!("WAV finalize error: {}", e))?;
    Ok(())
}

/// Load a WAV file and return mono f32 samples + sample rate.
/// Handles 16-bit, 24-bit, 32-bit int and 32-bit float formats.
pub fn load_wav(path: &std::path::Path) -> Result<(Vec<f32>, u32), String> {
    let reader = hound::WavReader::open(path).map_err(|e| format!("WAV open error: {}", e))?;
    let spec = reader.spec();
    let sample_rate = spec.sample_rate;
    let channels = spec.channels as usize;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .collect(),
    };

    // Mix down to mono if multi-channel
    let mono = if channels > 1 {
        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    Ok((mono, sample_rate))
}

/// Load an OGG/Vorbis file and return mono f32 samples + sample rate.
pub fn load_ogg(path: &std::path::Path) -> Result<(Vec<f32>, u32), String> {
    use lewton::inside_ogg::OggStreamReader;
    let file = std::fs::File::open(path).map_err(|e| format!("OGG open error: {}", e))?;
    let mut reader = OggStreamReader::new(file).map_err(|e| format!("OGG decode error: {}", e))?;
    let sample_rate = reader.ident_hdr.audio_sample_rate;
    let channels = reader.ident_hdr.audio_channels as usize;

    let mut interleaved: Vec<f32> = Vec::new();
    while let Some(pck) = reader
        .read_dec_packet_itl()
        .map_err(|e| format!("OGG packet error: {}", e))?
    {
        // lewton returns i16 interleaved samples; normalise to f32
        interleaved.extend(pck.iter().map(|&s| s as f32 / 32768.0));
    }

    // Mix down to mono
    let mono: Vec<f32> = if channels > 1 {
        interleaved
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        interleaved
    };

    Ok((mono, sample_rate))
}

/// Load any supported audio file (WAV or OGG/Vorbis) by extension.
/// Returns mono f32 samples + sample rate.
pub fn load_audio(path: &std::path::Path) -> Result<(Vec<f32>, u32), String> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("ogg") => load_ogg(path),
        _ => load_wav(path),
    }
}

/// Load any supported audio file and return interleaved f32 samples, channel count,
/// and sample rate. Used for stereo waveform display.
pub fn load_audio_interleaved(path: &std::path::Path) -> Result<(Vec<f32>, usize, u32), String> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("ogg") => {
            use lewton::inside_ogg::OggStreamReader;
            let file = std::fs::File::open(path).map_err(|e| format!("OGG open error: {}", e))?;
            let mut reader =
                OggStreamReader::new(file).map_err(|e| format!("OGG decode error: {}", e))?;
            let sample_rate = reader.ident_hdr.audio_sample_rate;
            let channels = reader.ident_hdr.audio_channels as usize;
            let mut interleaved: Vec<f32> = Vec::new();
            while let Some(pck) = reader
                .read_dec_packet_itl()
                .map_err(|e| format!("OGG packet error: {}", e))?
            {
                interleaved.extend(pck.iter().map(|&s| s as f32 / 32768.0));
            }
            Ok((interleaved, channels, sample_rate))
        }
        _ => {
            let reader =
                hound::WavReader::open(path).map_err(|e| format!("WAV open error: {}", e))?;
            let spec = reader.spec();
            let channels = spec.channels as usize;
            let sample_rate = spec.sample_rate;
            let samples: Vec<f32> = match spec.sample_format {
                hound::SampleFormat::Int => {
                    let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
                    reader
                        .into_samples::<i32>()
                        .filter_map(|s| s.ok())
                        .map(|s| s as f32 / max_val)
                        .collect()
                }
                hound::SampleFormat::Float => reader
                    .into_samples::<f32>()
                    .filter_map(|s| s.ok())
                    .collect(),
            };
            Ok((samples, channels, sample_rate))
        }
    }
}

// ── MIDI pitch → frequency ────────────────────────────────────────────

fn midi_to_freq(pitch: u8) -> f64 {
    // A4 = 69 = 440 Hz
    440.0 * crate::modules::fast_pow2((pitch as f64 - 69.0) / 12.0)
}

// ── Audio device enumeration ──────────────────────────────────────────

/// List available output device names. Index 0 is always "Default".
pub fn list_output_devices() -> Vec<String> {
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    let mut names = vec!["Default".to_string()];
    if let Ok(devices) = host.output_devices() {
        for dev in devices {
            if let Ok(name) = dev.name() {
                names.push(name);
            }
        }
    }
    names
}

// ── Start the audio engine ────────────────────────────────────────────

/// Start the realtime audio engine. Returns:
///   - SharedAudio: write transport + project data from the UI thread
///   - Arc<AtomicU64>: read the current playback position (as f64 bits)
pub fn start_audio_engine() -> Result<(SharedAudio, Arc<AtomicU64>), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let shared: SharedAudio = Arc::new(Mutex::new(AudioShared::default()));
    let shared_cb = shared.clone();

    // Atomic position feedback: audio → UI, no mutex needed on the read side
    let pos_atomic: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));
    let pos_cb = pos_atomic.clone();

    std::thread::spawn(move || {
        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                eprintln!("[audio] No output device");
                return;
            }
        };
        let config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[audio] Config error: {}", e);
                return;
            }
        };

        let sample_rate = config.sample_rate().0 as f64;
        // Write sample rate back
        if let Ok(mut s) = shared_cb.try_lock() {
            s.sample_rate = sample_rate;
        }

        let channels = config.channels() as usize;

        // Per-callback mutable state (lives only in the audio thread)
        let mut voices: Vec<ModuleVoice> = Vec::new();
        let mut prev_snapshot: Option<AudioShared> = None;
        // Persistent per-track instrument + effect instances.
        // Keyed by track index.  Rebuilt when module names change.
        let mut track_instruments: Vec<Option<(String, Box<dyn InstrumentModule>)>> = Vec::new();
        let mut track_effects: Vec<Vec<(String, Box<dyn EffectModule>)>> = Vec::new();
        // Per-track CStrip2 channel-strip instances (always-present, one per track).
        let mut track_cstrip: Vec<Box<dyn EffectModule>> = Vec::new();
        // Persistent per-track MIDI effect instances (one Vec per track).
        // Rebuilt when midi_effect_slots change, like track_effects above.
        let mut track_midi_effects: Vec<Vec<Box<dyn MidiEffect>>> = Vec::new();
        // Persistent master rack effect instances (applied to the stereo mix).
        let mut master_effects: Vec<(String, Box<dyn EffectModule>)> = Vec::new();

        // When transport stops, effects with tails (reverb, delay) keep
        // draining for up to this many samples so their tail rings out
        // instead of being abruptly cut.
        let mut tail_frames_remaining: usize = 0;
        let mut was_playing = false;

        // Anti-click fade: equal-power sine ramp 0→1 after seek/loop boundary.
        // 220 samples ≈ 5 ms @ 44.1kHz — below human perception of envelope shape,
        // removes discontinuity pops with zero tonal impact.
        const ANTI_CLICK_SAMPLES: usize = 220;
        let mut anti_click_remaining: usize = 0;

        // DC-offset high-pass filter state (one-pole IIR, fc ≈ 20 Hz).
        // Removes any DC bias that can push the waveform off-centre and cause
        // clicks when combined with abrupt gain changes.  Coefficient:
        //   R = 1 - 2π·fc/sr ≈ 0.99997 @ 44.1kHz  (close enough for any SR)
        const DC_HP_R: f64 = 0.99972; // fc ≈ 20 Hz @ 44.1kHz
        let mut dc_hp_x_l: f64 = 0.0; // prev input  — L
        let mut dc_hp_y_l: f64 = 0.0; // prev output — L
        let mut dc_hp_x_r: f64 = 0.0;
        let mut dc_hp_y_r: f64 = 0.0;

        // Slew rate limiter state: tracks previous master output sample.
        // Caps the per-sample change to SLEW_MAX, catching runaway spikes
        // that slip through all other protections. Threshold is set high
        // enough to never affect legitimate transients (e.g. snare drums).
        let mut slew_prev_l: f64 = 0.0;
        let mut slew_prev_r: f64 = 0.0;

        // Per-track smoothed pan + volume (one-pole, ~5 ms time constant).
        // Eliminates zipper noise from pan/volume automation without audible lag.
        let mut smooth_pan: Vec<f64> = Vec::new();
        let mut smooth_vol: Vec<f64> = Vec::new();

        // Smoothed master volume (one-pole) — prevents clicks when master fader moves.
        let mut smooth_master_vol: f64 = -1.0; // -1 sentinel → initialise on first use

        // Smoothed effect parameter cache: [track][slot] → Vec<(param_name, smoothed_value)>.
        // One-pole-filters each knob value so moving a knob during playback can't
        // cause a discontinuity in the DSP signal. Updated every callback before the
        // per-sample loop; the DSP reads these instead of the raw snapshot values.
        // Strings are allocated once when slots first appear and never re-allocated.
        // Also covers master rack: smooth_master_fx_params[slot].
        let mut smooth_track_fx_params: Vec<Vec<Vec<(String, f32)>>> = Vec::new();
        let mut smooth_master_fx_params: Vec<Vec<(String, f32)>> = Vec::new();
        // Per-track CStrip2 param smoothing cache: [track] → Vec<(param_name, smoothed_value)>
        let mut smooth_cstrip_params: Vec<Vec<(String, f32)>> = Vec::new();
        // Coefficient: ~1 ms @ 44.1kHz (same as pan/vol smoothing)
        const FX_SMOOTH_COEFF: f32 = 0.002;

        // Per-track arp state: (step_index, last_step_beat, held_pitches)
        let mut track_arp_state: Vec<(usize, f64, Vec<u8>)> = Vec::new();
        // Independent beat clock for keyboard/preview arp (always advances at BPM speed,
        // separate from the transport position so the arp works when transport is stopped).
        let mut keyboard_arp_beat: f64 = 0.0;
        // Per-track keyboard arp state: (step_index, last_beat)
        let mut keyboard_arp_state: Vec<(usize, f64)> = Vec::new();

        // ── Pre-allocated RT-safe scratch buffers ─────────────────────────────
        // These are allocated ONCE here (outside the callback closure) and reused
        // every callback invocation with .clear()/.resize() — no heap allocation
        // inside the hot audio path.
        let mut cb_pending_preview: Vec<(usize, u8, u8)> = Vec::with_capacity(16);
        let mut cb_pending_note_offs: Vec<u8> = Vec::with_capacity(16);
        let mut cb_cur_held_pitches: Vec<(usize, Vec<u8>)> = Vec::with_capacity(8);
        // Output sample buffers — resized only when frame count changes (rare).
        let mut cb_frame_l: Vec<f32> = Vec::with_capacity(2048);
        let mut cb_frame_r: Vec<f32> = Vec::with_capacity(2048);
        // Per-track accumulators — resized only when track count changes (rare).
        let mut cb_per_track_sample: Vec<(f64, f64)> = Vec::with_capacity(64);
        let mut cb_per_track_voices: Vec<usize> = Vec::with_capacity(64);
        let mut cb_track_rms: Vec<f32> = Vec::with_capacity(64);
        let mut cb_track_rms_pre: Vec<f32> = Vec::with_capacity(64);
        let mut cb_track_rms_l: Vec<f32> = Vec::with_capacity(64);
        let mut cb_track_rms_r: Vec<f32> = Vec::with_capacity(64);
        let mut cb_preview_rms: Vec<f32> = Vec::with_capacity(64);
        let mut cb_preview_rms_pre: Vec<f32> = Vec::with_capacity(64);
        // Arp scratch — reused per-track per-sample.
        let mut cb_arp_active_pitches: Vec<u8> = Vec::with_capacity(32);
        let mut cb_arp_pool: Vec<u8> = Vec::with_capacity(64);

        let stream = match device.build_output_stream(
            &config.into(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Take a non-blocking snapshot of shared state.
                // If the mutex is contended, reuse the previous snapshot.
                // Reuse pre-allocated scratch vecs — no heap allocation here.
                cb_pending_preview.clear();
                cb_pending_note_offs.clear();
                cb_cur_held_pitches.clear();
                let pending_preview = &mut cb_pending_preview;
                let pending_note_offs = &mut cb_pending_note_offs;
                let cur_held_pitches = &mut cb_cur_held_pitches;
                let mut sustain_mode = false;
                let snap: AudioShared = match shared_cb.try_lock() {
                    Ok(mut s) => {
                        // If UI requested a seek, jump to that position, clear voices,
                        // clear the flag, and update the atomic immediately so we play
                        // from the correct spot.
                        if s.seek_pending {
                            s.seek_pending = false;
                            voices.clear();
                            // Flush all time-based effect buffers so reverb/delay tails
                            // from before the seek don't bleed into the new playback position.
                            for track_fx in track_effects.iter_mut() {
                                for (_, fx) in track_fx.iter_mut() {
                                    fx.reset();
                                }
                            }
                            for (_, fx) in master_effects.iter_mut() {
                                fx.reset();
                            }
                            // Reset arp state so the arp re-fires from the new position.
                            for arp in track_arp_state.iter_mut() {
                                arp.0 = 0;
                                arp.1 = -999.0;
                                arp.2.clear();
                            }
                            // Reset MIDI effect state (e.g. arp step counters) on seek.
                            for chain in track_midi_effects.iter_mut() {
                                for fx in chain.iter_mut() {
                                    fx.reset();
                                }
                            }
                            let target = s.position_beats;
                            pos_cb.store(target.to_bits(), Ordering::Relaxed);
                            // Start equal-power fade-in to prevent pop at new position
                            anti_click_remaining = ANTI_CLICK_SAMPLES;
                        }
                        // Panic: kill all voices immediately
                        if s.panic {
                            s.panic = false;
                            voices.clear();
                            s.preview_playing = false;
                        }
                        // Extract preview note request — drain into pre-allocated vec (no alloc)
                        pending_preview.extend(s.preview_notes.drain(..));
                        // Extract note-offs — drain into pre-allocated vec (no alloc)
                        pending_note_offs.extend(s.preview_note_off.drain(..));
                        sustain_mode = s.preview_sustain;
                        // Snapshot the currently-held keyboard pitches (for keyboard arp).
                        // Clone is small (a few u8 vecs) and happens only when lock succeeds.
                        cur_held_pitches.extend(s.preview_held_pitches.iter().cloned());
                        let snap = s.clone();
                        prev_snapshot = Some(snap.clone());
                        snap
                    }
                    Err(_) => {
                        // UI thread is writing — reuse last good snapshot
                        match &prev_snapshot {
                            Some(p) => p.clone(),
                            None => return, // not started yet
                        }
                    }
                };

                // Handle note-off signals (piano key released).
                // Match by original_pitch so MIDI-effect-expanded voices (e.g. Chord)
                // are also released even though their .pitch differs from the key pressed.
                for off_pitch in pending_note_offs.iter() {
                    for voice in voices.iter_mut() {
                        if (voice.original_pitch == *off_pitch || voice.pitch == *off_pitch)
                            && !voice.released
                        {
                            voice.released = true;
                        }
                    }
                }

                // Handle preview notes from UI (piano key clicks / keyboard mode).
                // For tracks with an Arpeggiator, pressing a key updates the keyboard arp held
                // set and lets the keyboard arp clock fire voices; no direct spawn here.
                // For all other MIDI effects (Chord, Transpose, Velocity) spawn voices normally.
                for (preview_ti, preview_pitch, preview_vel) in pending_preview.iter() {
                    if *preview_ti < snap.tracks.len() {
                        let track = &snap.tracks[*preview_ti];
                        let vel = *preview_vel as f32 / 127.0;

                        // Ensure MIDI effect instances exist for this track.
                        while track_midi_effects.len() <= *preview_ti {
                            track_midi_effects.push(Vec::new());
                        }
                        let midi_changed = track.midi_effect_slots.len()
                            != track_midi_effects[*preview_ti].len()
                            || track
                                .midi_effect_slots
                                .iter()
                                .zip(track_midi_effects[*preview_ti].iter())
                                .any(|((want, _), have)| want != have.name());
                        if midi_changed {
                            let mut new_mfx: Vec<Box<dyn MidiEffect>> = Vec::new();
                            for (fx_name, _) in &track.midi_effect_slots {
                                let existing = track_midi_effects[*preview_ti]
                                    .iter()
                                    .position(|m| m.name() == fx_name.as_str());
                                if let Some(i) = existing {
                                    new_mfx.push(track_midi_effects[*preview_ti].remove(i));
                                } else if let Some(m) = create_midi_effect(fx_name) {
                                    new_mfx.push(m);
                                }
                            }
                            track_midi_effects[*preview_ti] = new_mfx;
                        }

                        // Check if this track has an arp in its MIDI chain.
                        let has_arp = track_midi_effects[*preview_ti]
                            .iter()
                            .any(|m| m.manages_voices());

                        if has_arp {
                            // For arp: the keyboard_arp clock will fire voices.
                            // Key-press just ensures the held set is up to date (main.rs sends it).
                            // Grow keyboard_arp_state as needed and reset beat so it fires immediately.
                            while keyboard_arp_state.len() <= *preview_ti {
                                keyboard_arp_state.push((0, -999.0));
                            }
                            // On first key-down (held was empty), reset so arp fires from beat 0.
                            keyboard_arp_state[*preview_ti].1 = -999.0;
                        } else {
                            // No arp — run MIDI chain and spawn voices directly.
                            let seed_events = vec![MidiEvent::new(*preview_pitch, vel)];
                            let final_events = run_midi_chain(
                                seed_events,
                                &mut track_midi_effects[*preview_ti],
                                track.midi_effect_slots.iter().map(|(_, p)| p),
                                0.0,
                                0.0,
                                snap.bpm,
                                sample_rate,
                            );

                            // Spawn voices for all resulting notes (after MIDI effects)
                            for ev in final_events {
                                let pitch = ev.pitch;
                                let final_vel = ev.velocity;
                                // Remove any existing preview voices for this track+pitch
                                voices
                                    .retain(|v| !(v.track_idx == *preview_ti && v.pitch == pitch));
                                let mut new_voice = ModuleVoice::new(
                                    midi_to_freq(pitch),
                                    final_vel,
                                    *preview_ti,
                                    pitch,
                                );
                                // Keep original_pitch = the key the user pressed so note-off
                                // (which sends preview_pitch) can release all chord-expanded voices.
                                new_voice.original_pitch = *preview_pitch;
                                voices.push(new_voice);
                                // In sustain mode (piano keyboard mode), don't auto-release — wait for key-up
                                // In normal mode (click preview), auto-release after 300ms
                                if !sustain_mode {
                                    if let Some(v) = voices.last_mut() {
                                        v.preview_samples_remaining =
                                            Some((sample_rate * 0.3) as u64);
                                    }
                                }
                            }
                        }
                    }
                }

                // Detect transport stop transition → start draining effect tails
                if was_playing && !snap.playing {
                    // Allow up to 6 seconds of tail drain (reverb, delay, etc.)
                    tail_frames_remaining = (sample_rate * 6.0) as usize;
                    // Release all non-preview voices so synths enter their
                    // release phase instead of sustaining forever.
                    for voice in voices.iter_mut() {
                        if voice.preview_samples_remaining.is_none() && !voice.released {
                            voice.released = true;
                        }
                    }
                }
                // Detect transport start transition → reset arp state so last_beat doesn't
                // refer to a position from a previous play session, causing the arp to
                // skip firing until pos catches up past the old last_beat.
                if !was_playing && snap.playing {
                    for arp in track_arp_state.iter_mut() {
                        arp.0 = 0;
                        arp.1 = -999.0;
                        arp.2.clear();
                    }
                    for chain in track_midi_effects.iter_mut() {
                        for fx in chain.iter_mut() {
                            fx.reset();
                        }
                    }
                }
                was_playing = snap.playing;

                if !snap.playing {
                    // Transport not playing — silence the transport output
                    // but still process sample preview, note preview voices,
                    // and drain any remaining effect tails.
                    let frames = data.len() / channels;
                    // Stereo preview frame buffer — reuse pre-allocated scratch (no alloc)
                    cb_frame_l.resize(frames, 0.0_f32);
                    cb_frame_r.resize(frames, 0.0_f32);
                    for v in cb_frame_l.iter_mut().take(frames) { *v = 0.0; }
                    for v in cb_frame_r.iter_mut().take(frames) { *v = 0.0; }
                    // Alias as a zipped iterator-friendly form via index access below.
                    let frame_samples_l = &mut cb_frame_l;
                    let frame_samples_r = &mut cb_frame_r;

                    // ── Preview sample playback (plays even when transport is stopped) ──
                    let mut preview_pos_local = snap.preview_pos;
                    if snap.preview_playing && !snap.preview_samples.is_empty() {
                        let preview_ratio = snap.preview_sample_rate as f64 / sample_rate;
                        let preview_end = snap.preview_end_sample; // 0 = play to file end
                        let loop_enabled = snap.preview_loop_enabled;
                        let loop_start = snap.preview_loop_start;
                        for fi in 0..frames {
                            // Check end boundary
                            if preview_end > 0 && preview_pos_local >= preview_end {
                                if loop_enabled {
                                    preview_pos_local = loop_start;
                                } else {
                                    break;
                                }
                            }
                            let src_pos = preview_pos_local as f64 * preview_ratio;
                            let src_idx = src_pos as usize;
                            if src_idx >= snap.preview_samples.len() {
                                if loop_enabled {
                                    preview_pos_local = loop_start;
                                    let src_pos2 = preview_pos_local as f64 * preview_ratio;
                                    let src_idx2 = src_pos2 as usize;
                                    if src_idx2 >= snap.preview_samples.len() {
                                        break;
                                    }
                                    let s0 = snap.preview_samples[src_idx2];
                                    let s1 = if src_idx2 + 1 < snap.preview_samples.len() {
                                        snap.preview_samples[src_idx2 + 1]
                                    } else {
                                        s0
                                    };
                                    let frac = (src_pos2 - src_pos2.floor()) as f32;
                                    let interp = s0 + (s1 - s0) * frac;
                                    let ps = (interp * snap.master_volume).clamp(-1.0, 1.0);
                                    frame_samples_l[fi] = ps;
                                    frame_samples_r[fi] = ps;
                                    preview_pos_local += 1;
                                    continue;
                                } else {
                                    break;
                                }
                            }
                            let s0 = snap.preview_samples[src_idx];
                            let s1 = if src_idx + 1 < snap.preview_samples.len() {
                                snap.preview_samples[src_idx + 1]
                            } else {
                                s0
                            };
                            let frac = (src_pos - src_pos.floor()) as f32;
                            let interp = s0 + (s1 - s0) * frac;
                            let ps = (interp * snap.master_volume).clamp(-1.0, 1.0);
                            // Preview samples are mono — write equally to both channels
                            frame_samples_l[fi] = ps;
                            frame_samples_r[fi] = ps;
                            preview_pos_local += 1;
                        }
                    }

                    // ── Process preview voices (note preview) even when stopped ──
                    let num_tracks = snap.tracks.len();
                    // Ensure instrument instances are up to date
                    while track_instruments.len() < num_tracks {
                        track_instruments.push(None);
                    }
                    while track_effects.len() < num_tracks {
                        track_effects.push(Vec::new());
                    }
                    while track_cstrip.len() < num_tracks {
                        track_cstrip.push(create_effect("CStrip2", sample_rate as u32).unwrap());
                    }
                    for (ti, track) in snap.tracks.iter().enumerate() {
                        if ti >= track_instruments.len() {
                            break;
                        }
                        let want_name = track.instrument_module.as_deref();
                        let have_name = track_instruments[ti].as_ref().map(|(n, _)| n.as_str());
                        if want_name != have_name {
                            track_instruments[ti] = want_name
                                .and_then(|n| create_instrument(n).map(|m| (n.to_string(), m)));
                        }
                        // Sync effect chain instances
                        let effects_changed = track.effect_slots.len() != track_effects[ti].len()
                            || track
                                .effect_slots
                                .iter()
                                .zip(track_effects[ti].iter())
                                .any(|((want, _), (have, _))| want != have);
                        if effects_changed {
                            let mut new_fx = Vec::new();
                            for (fx_name, _) in &track.effect_slots {
                                let idx = track_effects[ti]
                                    .iter()
                                    .position(|(n, _)| n.as_str() == fx_name.as_str());
                                if let Some(i) = idx {
                                    new_fx.push(track_effects[ti].remove(i));
                                } else if let Some(m) = create_effect(fx_name, sample_rate as u32) {
                                    new_fx.push((fx_name.to_string(), m));
                                }
                            }
                            track_effects[ti] = new_fx;
                        }
                    }

                    // Process each voice sample-by-sample — use pre-allocated scratch bufs
                    cb_preview_rms.clear();
                    cb_preview_rms.resize(num_tracks, 0.0_f32);
                    cb_preview_rms_pre.clear();
                    cb_preview_rms_pre.resize(num_tracks, 0.0_f32);
                    let preview_rms_accum = &mut cb_preview_rms;
                    let preview_rms_pre_accum = &mut cb_preview_rms_pre;

                    // Per-track accumulators for the inner loop
                    cb_per_track_sample.clear();
                    cb_per_track_sample.resize(num_tracks, (0.0_f64, 0.0_f64));
                    cb_per_track_voices.clear();
                    cb_per_track_voices.resize(num_tracks, 0usize);

                    let beats_per_sample_kbd = snap.bpm / 60.0 / sample_rate;
                    #[allow(clippy::needless_range_loop)]
                    for fi in 0..frames {
                        // ── Keyboard arp clock ──────────────────────────────────────────
                        keyboard_arp_beat += beats_per_sample_kbd;
                        while keyboard_arp_state.len() < snap.tracks.len() {
                            keyboard_arp_state.push((0, -999.0));
                        }
                        for (ti, track) in snap.tracks.iter().enumerate() {
                            let has_arp = ti < track_midi_effects.len()
                                && track_midi_effects[ti].iter().any(|m| m.manages_voices());
                            if !has_arp {
                                continue;
                            }
                            let held_opt = cur_held_pitches.iter().find(|(t, _)| *t == ti);
                            if held_opt.map_or(true, |(_, v)| v.is_empty()) {
                                for v in voices.iter_mut() {
                                    if v.track_idx == ti
                                        && !v.released
                                        && v.preview_samples_remaining.is_none()
                                    {
                                        v.released = true;
                                    }
                                }
                                keyboard_arp_state[ti] = (0, -999.0);
                                continue;
                            }
                            let held = held_opt.unwrap().1.as_slice();
                            let arp_params_opt = track
                                .midi_effect_slots
                                .iter()
                                .find(|(n, _)| n == "Arpeggiator")
                                .map(|(_, p)| p);
                            if let Some(arp_params) = arp_params_opt {
                                let get_arp = |k: &str, def: f32| -> f32 {
                                    arp_params
                                        .iter()
                                        .find(|(id, _)| id == k)
                                        .map(|(_, v)| *v)
                                        .unwrap_or(def)
                                };
                                let rate_beats = get_arp("rate", 0.25) as f64;
                                let octaves = get_arp("octaves", 1.0) as i32;
                                let pattern = get_arp("pattern", 0.0) as i32;
                                let vel_default = track.volume;
                                let (ref mut step, ref mut last_beat) = keyboard_arp_state[ti];

                                // Build pool — reuse scratch vec
                                cb_arp_pool.clear();
                                for oct in 0..octaves {
                                    for &p in held {
                                        cb_arp_pool.push((p as i32 + oct * 12).clamp(0, 127) as u8);
                                    }
                                }
                                match pattern {
                                    1 => cb_arp_pool.reverse(),
                                    2 => {
                                        let down_start = cb_arp_pool.len();
                                        // append reversed interior
                                        let mid: Vec<u8> = cb_arp_pool[1..down_start.saturating_sub(1)].iter().rev().copied().collect();
                                        cb_arp_pool.extend_from_slice(&mid);
                                    }
                                    3 => {
                                        let seed = (keyboard_arp_beat * 1000.0) as u64;
                                        for i in (1..cb_arp_pool.len()).rev() {
                                            let j = (seed
                                                .wrapping_mul(6364136223846793005)
                                                .wrapping_add(1442695040888963407)
                                                >> 33)
                                                as usize
                                                % (i + 1);
                                            cb_arp_pool.swap(i, j);
                                        }
                                    }
                                    _ => {}
                                }
                                if cb_arp_pool.is_empty() {
                                    continue;
                                }

                                let fire = if *last_beat < 0.0 {
                                    true
                                } else {
                                    let steps_now =
                                        (keyboard_arp_beat / rate_beats).floor() as usize;
                                    let steps_last = (*last_beat / rate_beats).floor() as usize;
                                    steps_now > steps_last
                                };
                                if fire {
                                    for v in voices.iter_mut() {
                                        if v.track_idx == ti
                                            && !v.released
                                            && v.preview_samples_remaining.is_none()
                                        {
                                            v.released = true;
                                        }
                                    }
                                    let idx = *step % cb_arp_pool.len();
                                    let pitch = cb_arp_pool[idx];
                                    voices.push(ModuleVoice::new(
                                        midi_to_freq(pitch),
                                        vel_default,
                                        ti,
                                        pitch,
                                    ));
                                    *step = (*step + 1) % cb_arp_pool.len().max(1);
                                    *last_beat = keyboard_arp_beat;
                                }
                            }
                        }

                        // Auto-release countdown for preview voices
                        for voice in voices.iter_mut() {
                            if let Some(remaining) = voice.preview_samples_remaining.as_mut() {
                                if *remaining == 0 {
                                    voice.released = true;
                                    voice.preview_samples_remaining = None;
                                } else {
                                    *remaining -= 1;
                                }
                            }
                        }
                        // Accumulate per-track stereo samples — reset scratch bufs
                        for v in cb_per_track_sample.iter_mut() { *v = (0.0, 0.0); }
                        for v in cb_per_track_voices.iter_mut() { *v = 0; }
                        for voice in voices.iter_mut() {
                            let ti = voice.track_idx;
                            if ti >= num_tracks {
                                continue;
                            }
                            let track = &snap.tracks[ti];
                            if let Some((_, ref instrument)) = track_instruments[ti] {
                                let (sl, sr) = instrument.process_voice(
                                    voice,
                                    &track.instrument_params,
                                    sample_rate,
                                    &track.extra,
                                );
                                cb_per_track_sample[ti].0 += sl;
                                cb_per_track_sample[ti].1 += sr;
                                cb_per_track_voices[ti] += 1;
                            }
                        }
                        // Normalize + run effect chain per track (same as main path)
                        for ti in 0..num_tracks {
                            if cb_per_track_sample[ti] == (0.0, 0.0) && cb_per_track_voices[ti] == 0 {
                                let has_tail = ti < track_effects.len()
                                    && track_effects[ti].iter().any(|(_, fx)| fx.has_tail());
                                if !has_tail {
                                    continue;
                                }
                            }
                            if cb_per_track_voices[ti] > 0 {
                                let norm = (cb_per_track_voices[ti] as f64).sqrt();
                                cb_per_track_sample[ti].0 /= norm;
                                cb_per_track_sample[ti].1 /= norm;
                            }
                            let pre_mono =
                                ((cb_per_track_sample[ti].0 + cb_per_track_sample[ti].1) * 0.5) as f32;
                            if ti < preview_rms_pre_accum.len() {
                                preview_rms_pre_accum[ti] += pre_mono * pre_mono;
                            }
                            if ti < track_effects.len() {
                                let track = &snap.tracks[ti];
                                for (fi2, (_, fx_params)) in track.effect_slots.iter().enumerate() {
                                    if fi2 < track_effects[ti].len() {
                                        track_effects[ti][fi2].1.set_bpm(snap.bpm);
                                        // Use pre-smoothed params if available, else raw snapshot
                                        let params_ref: &[(String, f32)] = if ti < smooth_track_fx_params.len()
                                            && fi2 < smooth_track_fx_params[ti].len()
                                            && smooth_track_fx_params[ti][fi2].len() == fx_params.len()
                                        {
                                            &smooth_track_fx_params[ti][fi2]
                                        } else {
                                            fx_params
                                        };
                                        let (ol, or2) = track_effects[ti][fi2].1.process(
                                            cb_per_track_sample[ti].0,
                                            cb_per_track_sample[ti].1,
                                            params_ref,
                                            sample_rate,
                                        );
                                        cb_per_track_sample[ti] = (ol, or2);
                                    }
                                }
                            }
                            // ── CStrip2 channel strip ──
                            if ti < track_cstrip.len() {
                                let cs_raw = &snap.tracks[ti].cstrip2_params;
                                if !cs_raw.is_empty() {
                                    let cs_params: &[(String, f32)] =
                                        if ti < smooth_cstrip_params.len()
                                            && smooth_cstrip_params[ti].len() == cs_raw.len()
                                        {
                                            &smooth_cstrip_params[ti]
                                        } else {
                                            cs_raw
                                        };
                                    let (cl, cr) = track_cstrip[ti].process(
                                        cb_per_track_sample[ti].0,
                                        cb_per_track_sample[ti].1,
                                        cs_params,
                                        sample_rate,
                                    );
                                    cb_per_track_sample[ti] = (cl, cr);
                                }
                            }
                            let post_mono =
                                ((cb_per_track_sample[ti].0 + cb_per_track_sample[ti].1) * 0.5) as f32;
                            if ti < preview_rms_accum.len() {
                                preview_rms_accum[ti] += post_mono * post_mono;
                            }
                        }
                        // Mix all tracks to stereo with equal-power panning
                        let mut mix_l = 0.0_f64;
                        let mut mix_r = 0.0_f64;
                        for (ti, track) in snap.tracks.iter().enumerate() {
                            if ti >= num_tracks {
                                break;
                            }
                            let (tl, tr) = cb_per_track_sample[ti];
                            let theta =
                                ((track.pan as f64) + 1.0) * 0.5 * std::f64::consts::FRAC_PI_2;
                            let pan_l = crate::modules::fast_cos(theta);
                            let pan_r = crate::modules::fast_sin(theta);
                            mix_l += tl * pan_l * track.volume as f64;
                            mix_r += tr * pan_r * track.volume as f64;
                        }
                        // Apply master rack effects to preview stereo mix
                        for (fi2, (_, fx_params)) in snap.master_effects.iter().enumerate() {
                            if fi2 < master_effects.len() {
                                master_effects[fi2].1.set_bpm(snap.bpm);
                                let params_ref: &[(String, f32)] = if fi2 < smooth_master_fx_params.len()
                                    && smooth_master_fx_params[fi2].len() == fx_params.len()
                                {
                                    &smooth_master_fx_params[fi2]
                                } else {
                                    fx_params
                                };
                                let (ml, mr) = master_effects[fi2].1.process_sidechain(
                                    mix_l,
                                    mix_r,
                                    mix_l,
                                    mix_r,
                                    params_ref,
                                    sample_rate,
                                );
                                mix_l = ml;
                                mix_r = mr;
                            }
                        }
                        let mv = smooth_master_vol;
                        frame_samples_l[fi] =
                            (frame_samples_l[fi] as f64 + mix_l * mv).clamp(-1.0, 1.0) as f32;
                        frame_samples_r[fi] =
                            (frame_samples_r[fi] as f64 + mix_r * mv).clamp(-1.0, 1.0) as f32;
                    }
                    // Remove dead voices
                    voices.retain(|v| !voice_is_done(v));

                    // Safety cap on voice count
                    const MAX_VOICES_STOPPED: usize = 256;
                    if voices.len() > MAX_VOICES_STOPPED {
                        voices.drain(0..voices.len() - MAX_VOICES_STOPPED);
                    }

                    // Count down tail drain timer
                    if tail_frames_remaining > 0 {
                        tail_frames_remaining = tail_frames_remaining.saturating_sub(frames);
                        if tail_frames_remaining == 0 {
                            for track_fx in track_effects.iter_mut() {
                                for (_, fx) in track_fx.iter_mut() {
                                    fx.reset();
                                }
                            }
                            for (_, fx) in master_effects.iter_mut() {
                                fx.reset();
                            }
                        }
                    }

                    // Expand to channels + write back (stereo)
                    for (fi, frame) in data.chunks_mut(channels).enumerate() {
                        let (l, r) = if fi < frames {
                            (frame_samples_l[fi], frame_samples_r[fi])
                        } else {
                            (0.0, 0.0)
                        };
                        if channels >= 2 {
                            frame[0] = l;
                            frame[1] = r;
                            for ch in frame.iter_mut().skip(2) {
                                *ch = 0.0;
                            }
                        } else {
                            frame[0] = (l + r) * 0.5;
                        }
                    }

                    // Write preview position back
                    if let Ok(mut s) = shared_cb.try_lock() {
                        s.preview_pos = preview_pos_local;
                        let preview_ratio = s.preview_sample_rate as f64 / sample_rate;
                        let src_idx = (preview_pos_local as f64 * preview_ratio) as usize;
                        let past_end =
                            s.preview_end_sample > 0 && preview_pos_local >= s.preview_end_sample;
                        if (src_idx >= s.preview_samples.len() || past_end)
                            && s.preview_playing
                            && !s.preview_loop_enabled
                        {
                            s.preview_playing = false;
                        }
                        let n = num_tracks;
                        if s.track_rms.len() != n {
                            s.track_rms.resize(n, 0.0);
                        }
                        if s.track_rms_pre_effect.len() != n {
                            s.track_rms_pre_effect.resize(n, 0.0);
                        }
                        if frames > 0 {
                            for (i, v) in s.track_rms.iter_mut().enumerate() {
                                let rms = if i < preview_rms_accum.len() {
                                    (preview_rms_accum[i] / frames as f32).sqrt()
                                } else {
                                    0.0
                                };
                                *v = *v * 0.85 + rms * 0.15;
                            }
                            for (i, v) in s.track_rms_pre_effect.iter_mut().enumerate() {
                                let rms = if i < preview_rms_pre_accum.len() {
                                    (preview_rms_pre_accum[i] / frames as f32).sqrt()
                                } else {
                                    0.0
                                };
                                *v = *v * 0.85 + rms * 0.15;
                            }
                        } else {
                            for v in s.track_rms.iter_mut() {
                                *v *= 0.85;
                                if *v < 0.0005 {
                                    *v = 0.0;
                                }
                            }
                        }
                        // Update oscilloscope (use L channel for display)
                        let osc_len = s.oscilloscope.len();
                        for &l in frame_samples_l.iter().take(frames) {
                            let w = s.osc_write % osc_len;
                            s.oscilloscope[w] = l;
                            s.osc_write += 1;
                        }
                        s.osc_write %= osc_len;
                    }

                    return;
                }

                let beats_per_sample = snap.bpm / 60.0 / sample_rate;
                let frames = data.len() / channels;

                // Check if any non-automation track is soloed.
                let any_solo = snap.tracks.iter().any(|t| t.solo && !t.is_automation);

                // Read current position from the atomic (authoritative; set by seek or prev callback)
                let pos_bits = pos_cb.load(Ordering::Relaxed);
                let mut pos = f64::from_bits(pos_bits);

                // Track whether this is the very first sample of a new playback/seek
                // Used to trigger notes that land exactly on the seek position
                let mut is_first_frame = true;

                // Process sample-by-sample to get sample-accurate loop wraparound
                // and exact clip boundary alignment.
                let frame_samples_l = {
                    cb_frame_l.resize(frames, 0.0_f32);
                    for v in &mut cb_frame_l[..frames] { *v = 0.0; }
                    &mut cb_frame_l
                };
                let frame_samples_r = {
                    cb_frame_r.resize(frames, 0.0_f32);
                    for v in &mut cb_frame_r[..frames] { *v = 0.0; }
                    &mut cb_frame_r
                };
                let num_tracks_total = snap.tracks.len();
                let track_rms_accum = {
                    cb_track_rms.resize(num_tracks_total, 0.0_f32);
                    for v in &mut cb_track_rms[..num_tracks_total] { *v = 0.0; }
                    &mut cb_track_rms
                };
                let track_rms_pre_accum = {
                    cb_track_rms_pre.resize(num_tracks_total, 0.0_f32);
                    for v in &mut cb_track_rms_pre[..num_tracks_total] { *v = 0.0; }
                    &mut cb_track_rms_pre
                };
                let track_rms_l_accum = {
                    cb_track_rms_l.resize(num_tracks_total, 0.0_f32);
                    for v in &mut cb_track_rms_l[..num_tracks_total] { *v = 0.0; }
                    &mut cb_track_rms_l
                };
                let track_rms_r_accum = {
                    cb_track_rms_r.resize(num_tracks_total, 0.0_f32);
                    for v in &mut cb_track_rms_r[..num_tracks_total] { *v = 0.0; }
                    &mut cb_track_rms_r
                };
                let mut master_rms_accum = 0.0_f32;
                let mut master_rms_pre_accum = 0.0_f32;
                let mut master_rms_l_accum = 0.0_f32;
                let mut master_rms_r_accum = 0.0_f32;
                let mut rms_frame_count = 0usize;

                // ── Sync instrument/effect instances ONCE per callback (not per sample) ──
                {
                    let num_tracks = snap.tracks.len();
                    while track_instruments.len() < num_tracks {
                        track_instruments.push(None);
                    }
                    while track_effects.len() < num_tracks {
                        track_effects.push(Vec::new());
                    }
                    while track_cstrip.len() < num_tracks {
                        track_cstrip.push(create_effect("CStrip2", sample_rate as u32).unwrap());
                    }
                    while track_midi_effects.len() < num_tracks {
                        track_midi_effects.push(Vec::new());
                    }
                    while track_arp_state.len() < num_tracks {
                        track_arp_state.push((0, -999.0, Vec::new()));
                    }
                    for (ti, track) in snap.tracks.iter().enumerate() {
                        if ti >= track_instruments.len() {
                            break;
                        }
                        let want_name = track.instrument_module.as_deref();
                        let have_name = track_instruments[ti].as_ref().map(|(n, _)| n.as_str());
                        if want_name != have_name {
                            track_instruments[ti] = want_name
                                .and_then(|n| create_instrument(n).map(|m| (n.to_string(), m)));
                        }
                        let effects_changed = track.effect_slots.len() != track_effects[ti].len()
                            || track
                                .effect_slots
                                .iter()
                                .zip(track_effects[ti].iter())
                                .any(|((want, _), (have, _))| want != have);
                        if effects_changed {
                            let mut new_fx = Vec::new();
                            for (fx_name, _) in &track.effect_slots {
                                let idx = track_effects[ti]
                                    .iter()
                                    .position(|(n, _)| n.as_str() == fx_name.as_str());
                                if let Some(i) = idx {
                                    new_fx.push(track_effects[ti].remove(i));
                                } else if let Some(m) = create_effect(fx_name, sample_rate as u32) {
                                    new_fx.push((fx_name.to_string(), m));
                                }
                            }
                            track_effects[ti] = new_fx;
                        }
                        // ── Sync MIDI effect instances ──
                        let midi_changed = track.midi_effect_slots.len()
                            != track_midi_effects[ti].len()
                            || track
                                .midi_effect_slots
                                .iter()
                                .zip(track_midi_effects[ti].iter())
                                .any(|((want, _), have)| want != have.name());
                        if midi_changed {
                            let mut new_mfx: Vec<Box<dyn MidiEffect>> = Vec::new();
                            for (fx_name, _) in &track.midi_effect_slots {
                                // Reuse existing instance if possible (preserves arp state)
                                let existing = track_midi_effects[ti]
                                    .iter()
                                    .position(|m| m.name() == fx_name.as_str());
                                if let Some(i) = existing {
                                    new_mfx.push(track_midi_effects[ti].remove(i));
                                } else if let Some(m) = create_midi_effect(fx_name) {
                                    new_mfx.push(m);
                                }
                            }
                            track_midi_effects[ti] = new_mfx;
                        }
                    }
                    // ── Sync master rack effect instances ──
                    {
                        let master_changed = snap.master_effects.len() != master_effects.len()
                            || snap
                                .master_effects
                                .iter()
                                .zip(master_effects.iter())
                                .any(|((want, _), (have, _))| want != have);
                        if master_changed {
                            let mut new_fx = Vec::new();
                            for (fx_name, _) in &snap.master_effects {
                                let idx = master_effects
                                    .iter()
                                    .position(|(n, _)| n.as_str() == fx_name.as_str());
                                if let Some(i) = idx {
                                    new_fx.push(master_effects.remove(i));
                                } else if let Some(m) = create_effect(fx_name, sample_rate as u32) {
                                    new_fx.push((fx_name.to_string(), m));
                                }
                            }
                            master_effects = new_fx;
                        }
                    }
                }

                // Pre-allocate per-track buffers outside the per-sample loop
                let num_tracks = snap.tracks.len();
                let per_track_sample = {
                    cb_per_track_sample.resize(num_tracks, (0.0_f64, 0.0_f64));
                    &mut cb_per_track_sample
                };
                let per_track_voices = {
                    cb_per_track_voices.resize(num_tracks, 0usize);
                    &mut cb_per_track_voices
                };
                // Hoist constant conversions out of the per-sample loop
                let beats_per_sec = snap.bpm / 60.0;

                // ── Per-callback parameter smoothing ──────────────────────────────
                // Smooth every effect parameter and master volume toward their
                // snapshot target values.  This runs once per audio callback
                // (not once per sample) so it's O(params) not O(params×samples).
                // Parameters are passed to DSP as references to these smoothed slices.
                // FX_SMOOTH_COEFF is deliberately applied per-buffer, which at
                // 512 samples/buffer @ 44.1kHz gives ~11.6 ms time constant —
                // fast enough that knobs feel instant but slow enough to prevent
                // any discontinuity.

                // Initialise master volume smoother on first use
                if smooth_master_vol < 0.0 {
                    smooth_master_vol = snap.master_volume as f64;
                }
                smooth_master_vol += (snap.master_volume as f64 - smooth_master_vol) * (FX_SMOOTH_COEFF as f64 * frames as f64).min(1.0);

                // Grow track fx param cache to match current layout
                while smooth_track_fx_params.len() < snap.tracks.len() {
                    smooth_track_fx_params.push(Vec::new());
                }
                for (ti, track) in snap.tracks.iter().enumerate() {
                    let slot_cache = &mut smooth_track_fx_params[ti];
                    while slot_cache.len() < track.effect_slots.len() {
                        slot_cache.push(Vec::new());
                    }
                    for (si, (_, params)) in track.effect_slots.iter().enumerate() {
                        let param_cache = &mut slot_cache[si];
                        // Initialise missing entries with (name, value) — allocates strings
                        // only ONCE when the slot first appears.
                        if param_cache.len() < params.len() {
                            for pi in param_cache.len()..params.len() {
                                param_cache.push(params[pi].clone());
                            }
                        }
                        // One-pole smooth every param value toward its target (no allocation)
                        let coeff = (FX_SMOOTH_COEFF * frames as f32).min(1.0);
                        for (pi, p) in params.iter().enumerate() {
                            param_cache[pi].1 += (p.1 - param_cache[pi].1) * coeff;
                        }
                    }
                }
                // Grow master fx param cache
                while smooth_master_fx_params.len() < snap.master_effects.len() {
                    smooth_master_fx_params.push(Vec::new());
                }
                for (si, (_, params)) in snap.master_effects.iter().enumerate() {
                    let param_cache = &mut smooth_master_fx_params[si];
                    if param_cache.len() < params.len() {
                        for pi in param_cache.len()..params.len() {
                            param_cache.push(params[pi].clone());
                        }
                    }
                    let coeff = (FX_SMOOTH_COEFF * frames as f32).min(1.0);
                    for (pi, p) in params.iter().enumerate() {
                        param_cache[pi].1 += (p.1 - param_cache[pi].1) * coeff;
                    }
                }

                // Smooth CStrip2 channel-strip params per track
                while smooth_cstrip_params.len() < snap.tracks.len() {
                    smooth_cstrip_params.push(Vec::new());
                }
                for (ti, track) in snap.tracks.iter().enumerate() {
                    let cs_raw = &track.cstrip2_params;
                    if !cs_raw.is_empty() {
                        let cache = &mut smooth_cstrip_params[ti];
                        if cache.len() < cs_raw.len() {
                            for pi in cache.len()..cs_raw.len() {
                                cache.push(cs_raw[pi].clone());
                            }
                        }
                        let coeff = (FX_SMOOTH_COEFF * frames as f32).min(1.0);
                        for (pi, p) in cs_raw.iter().enumerate() {
                            cache[pi].1 += (p.1 - cache[pi].1) * coeff;
                        }
                    }
                }

                for frame_idx in 0..frames {
                    // ── Loop wraparound (sample-accurate) ──
                    if snap.loop_enabled && pos >= snap.loop_end {
                        // Carry fractional overshoot past loop end into loop region
                        let overshoot = pos - snap.loop_end;
                        pos =
                            snap.loop_start + overshoot.rem_euclid(snap.loop_end - snap.loop_start);
                        // Kill all voices on loop boundary to avoid pitch artifacts
                        voices.clear();
                        // Reset arp state on loop boundary
                        for arp in track_arp_state.iter_mut() {
                            arp.0 = 0;
                            arp.1 = -999.0;
                            arp.2.clear();
                        }
                        // Start equal-power fade-in to prevent pop at loop boundary
                        anti_click_remaining = ANTI_CLICK_SAMPLES;
                    }

                    // ── Trigger new MIDI note voices ──
                    for (ti, track) in snap.tracks.iter().enumerate() {
                        if track.is_automation {
                            continue;
                        }
                        if track.mute {
                            continue;
                        }
                        if any_solo && !track.solo {
                            continue;
                        }
                        // Skip MIDI synthesis if track has no instrument module in its rack
                        if track.instrument_module.is_none() {
                            continue;
                        }
                        for clip in &track.midi_clips {
                            let clip_end = clip.start_beats + clip.length_beats;
                            if pos < clip.start_beats || pos >= clip_end {
                                continue;
                            }
                            let clip_pos = pos - clip.start_beats;
                            for note in &clip.notes {
                                let note_start = note.start_beats;
                                let prev_clip_pos = clip_pos - beats_per_sample;
                                let just_started =
                                    prev_clip_pos < note_start && clip_pos >= note_start;
                                let catch_on_start = is_first_frame
                                    && clip_pos >= note_start
                                    && clip_pos < note_start + beats_per_sample * 2.0
                                    && !voices.iter().any(|v| {
                                        v.track_idx == ti
                                            && v.original_pitch == note.pitch
                                            && !v.released
                                    });
                                if just_started || catch_on_start {
                                    let vel = note.velocity as f32 / 127.0 * track.volume;
                                    let seed_events = vec![MidiEvent::new(note.pitch, vel)];

                                    // Check if arp is in the chain — arp manages its own voices
                                    let has_arp = ti < track_midi_effects.len()
                                        && track_midi_effects[ti]
                                            .iter()
                                            .any(|m| m.manages_voices());

                                    if has_arp {
                                        // For arp: accumulate held notes; arp step logic fires below
                                        if ti < track_arp_state.len() {
                                            let held = &mut track_arp_state[ti].2;
                                            if !held.contains(&note.pitch) {
                                                held.push(note.pitch);
                                                held.sort_unstable();
                                            }
                                        }
                                    } else {
                                        // Run the full MIDI effect chain
                                        let final_events = if ti < track_midi_effects.len() {
                                            run_midi_chain(
                                                seed_events,
                                                &mut track_midi_effects[ti],
                                                track.midi_effect_slots.iter().map(|(_, p)| p),
                                                pos,
                                                pos - beats_per_sample,
                                                snap.bpm,
                                                sample_rate,
                                            )
                                        } else {
                                            vec![MidiEvent::new(note.pitch, vel)]
                                        };

                                        // Spawn voices for all resulting notes
                                        for ev in final_events {
                                            if !voices.iter().any(|v| {
                                                v.track_idx == ti
                                                    && v.pitch == ev.pitch
                                                    && !v.released
                                            }) {
                                                let freq = midi_to_freq(ev.pitch);
                                                let mut voice = ModuleVoice::new(
                                                    freq,
                                                    ev.velocity,
                                                    ti,
                                                    ev.pitch,
                                                );
                                                voice.original_pitch = note.pitch;
                                                voices.push(voice);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ── Arp per-track step trigger ──
                        // For tracks with an Arpeggiator, generate stepped voices.
                        let has_arp_instance = ti < track_midi_effects.len()
                            && track_midi_effects[ti].iter().any(|m| m.manages_voices());
                        if has_arp_instance {
                            // Find the arp params from the slot data
                            let arp_params_opt = track
                                .midi_effect_slots
                                .iter()
                                .find(|(n, _)| n == "Arpeggiator")
                                .map(|(_, p)| p);
                            if let Some(arp_params) = arp_params_opt {
                                let get_arp = |k: &str, default: f32| -> f32 {
                                    arp_params
                                        .iter()
                                        .find(|(id, _)| id == k)
                                        .map(|(_, v)| *v)
                                        .unwrap_or(default)
                                };
                                let rate_beats = get_arp("rate", 0.25) as f64;
                                let octaves = get_arp("octaves", 1.0) as i32;
                                let pattern = get_arp("pattern", 0.0) as i32;
                                let vel_default = track.volume;

                                if ti < track_arp_state.len() {
                                    let (ref mut step, ref mut last_beat, ref mut _held) =
                                        track_arp_state[ti];

                                    // Collect currently-active notes in all clips for this track
                                    cb_arp_active_pitches.clear();
                                    let active_pitches = &mut cb_arp_active_pitches;
                                    for clip in &track.midi_clips {
                                        let clip_end = clip.start_beats + clip.length_beats;
                                        if pos < clip.start_beats || pos >= clip_end {
                                            continue;
                                        }
                                        let clip_pos = pos - clip.start_beats;
                                        for note in &clip.notes {
                                            if clip_pos >= note.start_beats
                                                && clip_pos < note.start_beats + note.length_beats
                                                && !active_pitches.contains(&note.pitch)
                                            {
                                                active_pitches.push(note.pitch);
                                            }
                                        }
                                    }
                                    active_pitches.sort_unstable();

                                    // Build arp note pool across octaves
                                    cb_arp_pool.clear();
                                    let pool = &mut cb_arp_pool;
                                    for oct in 0..octaves {
                                        for &p in active_pitches.iter() {
                                            let shifted = (p as i32 + oct * 12).clamp(0, 127) as u8;
                                            pool.push(shifted);
                                        }
                                    }
                                    // Apply pattern
                                    match pattern {
                                        1 => pool.reverse(),
                                        2 => {
                                            let down_start = pool.len();
                                            let mid: Vec<u8> = pool[1..down_start.saturating_sub(1)].iter().rev().copied().collect();
                                            pool.extend_from_slice(&mid);
                                        }
                                        3 => {
                                            let seed = (pos * 1000.0) as u64;
                                            for i in (1..pool.len()).rev() {
                                                let j = (seed
                                                    .wrapping_mul(6364136223846793005)
                                                    .wrapping_add(1442695040888963407)
                                                    >> 33)
                                                    as usize
                                                    % (i + 1);
                                                pool.swap(i, j);
                                            }
                                        }
                                        _ => {}
                                    }

                                    if !pool.is_empty() {
                                        let fire = if *last_beat < 0.0 {
                                            true
                                        } else {
                                            let steps_now = (pos / rate_beats).floor() as usize;
                                            let steps_last =
                                                (*last_beat / rate_beats).floor() as usize;
                                            steps_now > steps_last
                                        };

                                        if fire {
                                            for v in voices.iter_mut() {
                                                if v.track_idx == ti && !v.released {
                                                    v.released = true;
                                                }
                                            }
                                            let pool_step = *step % pool.len();
                                            let pitch = pool[pool_step];
                                            let freq = midi_to_freq(pitch);
                                            voices.push(ModuleVoice::new(
                                                freq,
                                                vel_default,
                                                ti,
                                                pitch,
                                            ));
                                            *step = (*step + 1) % pool.len().max(1);
                                            *last_beat = pos;
                                        }
                                    } else {
                                        for v in voices.iter_mut() {
                                            if v.track_idx == ti && !v.released {
                                                v.released = true;
                                            }
                                        }
                                        *step = 0;
                                        *last_beat = -999.0;
                                    }
                                }
                            }
                        }
                    }

                    // ── Release / kill voices whose notes have ended ──

                    for voice in voices.iter_mut() {
                        if voice.released {
                            continue; // already in release
                        }
                        // Preview voices manage their own lifetime via preview_samples_remaining
                        if voice.preview_samples_remaining.is_some() {
                            continue;
                        }
                        let track = &snap.tracks[voice.track_idx];
                        // If the track is muted or not soloed, keep the voice alive
                        // (don't release it) so it resumes naturally when unmuted.
                        if track.is_automation || track.mute || (any_solo && !track.solo) {
                            continue;
                        }
                        // Tracks with arp-like effects manage their own voice lifecycle.
                        let has_arp = voice.track_idx < track_midi_effects.len()
                            && track_midi_effects[voice.track_idx]
                                .iter()
                                .any(|m| m.manages_voices());
                        if has_arp {
                            continue;
                        }
                        let mut still_active = false;
                        for clip in &track.midi_clips {
                            let clip_end = clip.start_beats + clip.length_beats;
                            if pos < clip.start_beats || pos >= clip_end {
                                continue;
                            }
                            let clip_pos = pos - clip.start_beats;
                            for note in &clip.notes {
                                if note.pitch == voice.original_pitch
                                    && clip_pos >= note.start_beats
                                    && clip_pos < note.start_beats + note.length_beats
                                {
                                    still_active = true;
                                    break;
                                }
                            }
                            if still_active {
                                break;
                            }
                        }
                        if !still_active {
                            voice.released = true;
                        }
                    }

                    // ── Auto-release preview voices after their timer expires ──
                    // Combined with the voice-is-done cull to reduce passes
                    for voice in voices.iter_mut() {
                        if let Some(remaining) = voice.preview_samples_remaining.as_mut() {
                            if *remaining == 0 {
                                voice.released = true;
                                voice.preview_samples_remaining = None;
                            } else {
                                *remaining -= 1;
                            }
                        }
                    }
                    // Remove fully finished voices (amp envelope done)
                    voices.retain(|v| !voice_is_done(v));

                    // Safety cap: prevent unbounded voice accumulation from causing
                    // CPU overload / glitches over time. Keep newest voices.
                    const MAX_VOICES: usize = 256;
                    if voices.len() > MAX_VOICES {
                        voices.drain(0..voices.len() - MAX_VOICES);
                    }

                    // ── Synthesize all voices via module trait objects ──
                    // Reset per-track accumulators (allocated outside loop)
                    per_track_sample.fill((0.0, 0.0));
                    per_track_voices.fill(0);
                    for voice in voices.iter_mut() {
                        let ti = voice.track_idx;
                        if ti >= num_tracks {
                            continue;
                        }
                        let track = &snap.tracks[ti];
                        if let Some((_, ref instrument)) = track_instruments[ti] {
                            let (sl, sr) = instrument.process_voice(
                                voice,
                                &track.instrument_params,
                                sample_rate,
                                &track.extra,
                            );
                            per_track_sample[ti].0 += sl;
                            per_track_sample[ti].1 += sr;
                            per_track_voices[ti] += 1;
                        }
                    }

                    // ── Mix audio clips into per-track mono ──
                    for (ti, track) in snap.tracks.iter().enumerate() {
                        if track.is_automation {
                            continue;
                        }
                        if track.mute {
                            continue;
                        }
                        if any_solo && !track.solo {
                            continue;
                        }
                        for aclip in &track.audio_clips {
                            let clip_end = aclip.start_beats + aclip.length_beats;
                            if pos < aclip.start_beats || pos >= clip_end {
                                continue;
                            }
                            // Position within the clip (in beats from clip start)
                            let clip_pos_beats = pos - aclip.start_beats;
                            // Convert beat position to seconds using pre-computed rate
                            let clip_pos_secs = clip_pos_beats / beats_per_sec;
                            // Add the offset (already in seconds) to get position in audio file
                            let audio_pos_secs = clip_pos_secs + aclip.offset_secs;
                            let src_pos = audio_pos_secs * aclip.sample_rate as f64;
                            let src_idx = src_pos as usize;
                            if src_idx < aclip.samples.len() {
                                // Linear interpolation between samples for click-free resampling
                                let frac = src_pos - src_pos.floor();
                                let s0 = aclip.samples[src_idx] as f64;
                                let s1 = if src_idx + 1 < aclip.samples.len() {
                                    aclip.samples[src_idx + 1] as f64
                                } else {
                                    s0
                                };
                                let mut s = (s0 + (s1 - s0) * frac)
                                    * aclip.gain as f64
                                    * track.volume as f64;

                                // Equal-power micro-fade at CLIP boundaries (~5 ms = 220 samples).
                                // Sine curve: gain = sin(t·π/2) removes click without tonal change.
                                // Applied only at the first/last ~5 ms of each clip — the rest plays
                                // at full gain, so musical content is completely unaffected.
                                let fade_len = 220usize;
                                let clip_sample = (clip_pos_secs * sample_rate) as usize;
                                let clip_len_samples =
                                    (aclip.length_beats / beats_per_sec * sample_rate) as usize;
                                // Fade in at clip start
                                if clip_sample < fade_len {
                                    let t = clip_sample as f64 / fade_len as f64;
                                    s *= (t * std::f64::consts::FRAC_PI_2).sin();
                                }
                                // Fade out at clip end
                                let remaining = clip_len_samples.saturating_sub(clip_sample);
                                if remaining < fade_len && fade_len > 0 {
                                    let t = remaining as f64 / fade_len as f64;
                                    s *= (t * std::f64::consts::FRAC_PI_2).sin();
                                }

                                // User-controlled fade-in
                                if aclip.fade_in > 0.0 {
                                    let fade_in_samples = (aclip.fade_in * sample_rate) as usize;
                                    if fade_in_samples > 0 && clip_sample < fade_in_samples {
                                        s *= clip_sample as f64 / fade_in_samples as f64;
                                    }
                                }
                                // User-controlled fade-out
                                if aclip.fade_out > 0.0 {
                                    let fade_out_samples = (aclip.fade_out * sample_rate) as usize;
                                    if fade_out_samples > 0 && clip_len_samples > fade_out_samples {
                                        let fade_out_start = clip_len_samples - fade_out_samples;
                                        if clip_sample >= fade_out_start {
                                            s *= (clip_len_samples - clip_sample) as f64
                                                / fade_out_samples as f64;
                                        }
                                    }
                                }

                                if ti < num_tracks {
                                    per_track_sample[ti].0 += s;
                                    per_track_sample[ti].1 += s;
                                }
                            }
                        }
                    }

                    // ── Run effect chain per track via trait objects ──
                    for (ti, track) in snap.tracks.iter().enumerate() {
                        if track.is_automation || ti >= num_tracks {
                            continue;
                        }
                        if per_track_sample[ti] == (0.0, 0.0) && per_track_voices[ti] == 0 {
                            let has_tail = ti < track_effects.len()
                                && track_effects[ti].iter().any(|(_, fx)| fx.has_tail());
                            if !has_tail {
                                continue;
                            }
                        }
                        // Normalize voices before effects
                        if per_track_voices[ti] > 0 {
                            let vc = per_track_voices[ti] as f64;
                            let norm = vc.sqrt();
                            per_track_sample[ti].0 /= norm;
                            per_track_sample[ti].1 /= norm;
                        }
                        // Capture pre-effect signal for GR meter
                        {
                            let (tl, tr) = per_track_sample[ti];
                            let ts = (tl + tr) * 0.5;
                            if ti < track_rms_pre_accum.len() {
                                track_rms_pre_accum[ti] += (ts * ts) as f32;
                            }
                        }
                        // Process through each effect module
                        for (fi, (_, fx_params)) in track.effect_slots.iter().enumerate() {
                            if fi < track_effects[ti].len() {
                                // Provide BPM to beat-synced effects (e.g. delay)
                                track_effects[ti][fi].1.set_bpm(snap.bpm);
                                // Resolve sidechain source signal (default: self)
                                let sc_ti = track.effect_sidechain_track.get(fi).copied().flatten();
                                let (key_l, key_r) = if let Some(sc_idx) = sc_ti {
                                    if sc_idx < per_track_sample.len() {
                                        per_track_sample[sc_idx]
                                    } else {
                                        per_track_sample[ti]
                                    }
                                } else {
                                    per_track_sample[ti]
                                };
                                // Use pre-smoothed params if available, else raw snapshot
                                let params_ref: &[(String, f32)] = if ti < smooth_track_fx_params.len()
                                    && fi < smooth_track_fx_params[ti].len()
                                    && smooth_track_fx_params[ti][fi].len() == fx_params.len()
                                {
                                    &smooth_track_fx_params[ti][fi]
                                } else {
                                    fx_params
                                };
                                let (ol, or2) = track_effects[ti][fi].1.process_sidechain(
                                    per_track_sample[ti].0,
                                    per_track_sample[ti].1,
                                    key_l,
                                    key_r,
                                    params_ref,
                                    sample_rate,
                                );
                                per_track_sample[ti] = (ol, or2);
                            }
                        }
                        // ── CStrip2 channel strip ──
                        if ti < track_cstrip.len() {
                            let cs_raw = &track.cstrip2_params;
                            if !cs_raw.is_empty() {
                                let cs_params: &[(String, f32)] =
                                    if ti < smooth_cstrip_params.len()
                                        && smooth_cstrip_params[ti].len() == cs_raw.len()
                                    {
                                        &smooth_cstrip_params[ti]
                                    } else {
                                        cs_raw
                                    };
                                let (cl, cr) = track_cstrip[ti].process(
                                    per_track_sample[ti].0,
                                    per_track_sample[ti].1,
                                    cs_params,
                                    sample_rate,
                                );
                                per_track_sample[ti] = (cl, cr);
                            }
                        }
                    }

                    // ── Apply per-track panning → stereo mix ──
                    // Equal-power panning: left = cos(θ), right = sin(θ)
                    // where θ = (pan + 1) / 2 * π/2   (pan: -1..1 → θ: 0..π/2)
                    // Pan is smoothed (one-pole, ~1 ms) to remove zipper noise on pan changes.
                    // Grow smoothing arrays to match track count
                    while smooth_pan.len() < num_tracks {
                        let ti = smooth_pan.len();
                        let init = if ti < snap.tracks.len() { snap.tracks[ti].pan as f64 } else { 0.0 };
                        smooth_pan.push(init);
                    }
                    while smooth_vol.len() < num_tracks {
                        let ti = smooth_vol.len();
                        let init = if ti < snap.tracks.len() { snap.tracks[ti].volume as f64 } else { 1.0 };
                        smooth_vol.push(init);
                    }
                    // One-pole smoothing coefficient — ~1 ms time constant @ 44.1kHz
                    const SMOOTH_COEFF: f64 = 0.002;
                    let mut mix_l = 0.0_f64;
                    let mut mix_r = 0.0_f64;
                    for (ti, track) in snap.tracks.iter().enumerate() {
                        if track.is_automation {
                            continue;
                        }
                        if track.mute {
                            continue;
                        }
                        if any_solo && !track.solo {
                            continue;
                        }
                        if ti >= num_tracks {
                            break;
                        }
                        // Step smoothed pan and volume toward targets (removes zipper noise)
                        smooth_pan[ti] += (track.pan as f64 - smooth_pan[ti]) * SMOOTH_COEFF;
                        smooth_vol[ti] += (track.volume as f64 - smooth_vol[ti]) * SMOOTH_COEFF;
                        let (tl, tr) = per_track_sample[ti];
                        // Equal-power pan (fast approximation) using smoothed pan
                        let theta = (smooth_pan[ti] + 1.0) * 0.5 * std::f64::consts::FRAC_PI_2;
                        let pan_l = crate::modules::fast_cos(theta);
                        let pan_r = crate::modules::fast_sin(theta);
                        mix_l += tl * pan_l * smooth_vol[ti];
                        mix_r += tr * pan_r * smooth_vol[ti];
                    }

                    // ── Apply master rack effects to the stereo mix ──
                    // Capture pre-effect master level
                    {
                        let ms = (mix_l + mix_r) * 0.5;
                        master_rms_pre_accum += (ms * ms) as f32;
                    }
                    for (fi, (_, fx_params)) in snap.master_effects.iter().enumerate() {
                        if fi < master_effects.len() {
                            master_effects[fi].1.set_bpm(snap.bpm);
                            let params_ref: &[(String, f32)] = if fi < smooth_master_fx_params.len()
                                && smooth_master_fx_params[fi].len() == fx_params.len()
                            {
                                &smooth_master_fx_params[fi]
                            } else {
                                fx_params
                            };
                            let (ml, mr) = master_effects[fi].1.process_sidechain(
                                mix_l,
                                mix_r,
                                mix_l,
                                mix_r,
                                params_ref,
                                sample_rate,
                            );
                            mix_l = ml;
                            mix_r = mr;
                        }
                    }
                    // Capture post-effect master level (before master volume)
                    {
                        let ms = (mix_l + mix_r) * 0.5;
                        master_rms_accum += (ms * ms) as f32;
                        master_rms_l_accum += (mix_l * mix_l) as f32;
                        master_rms_r_accum += (mix_r * mix_r) as f32;
                    }

                    mix_l *= smooth_master_vol;
                    mix_r *= smooth_master_vol;

                    // Anti-click fade: equal-power sine ramp up from 0 after seek/loop boundary
                    if anti_click_remaining > 0 {
                        // t ∈ [0, 1] → gain = sin(t * π/2)  (equal-power, no tonal change)
                        let t = 1.0 - (anti_click_remaining as f64 / ANTI_CLICK_SAMPLES as f64);
                        let gain = (t * std::f64::consts::FRAC_PI_2).sin();
                        mix_l *= gain;
                        mix_r *= gain;
                        anti_click_remaining -= 1;
                    }

                    // DC-offset removal: one-pole high-pass filter (fc ≈ 20 Hz).
                    // Removes any slowly-drifting bias without affecting audible content.
                    {
                        let new_l = mix_l - dc_hp_x_l + DC_HP_R * dc_hp_y_l;
                        dc_hp_x_l = mix_l;
                        dc_hp_y_l = new_l;
                        mix_l = new_l;
                        let new_r = mix_r - dc_hp_x_r + DC_HP_R * dc_hp_y_r;
                        dc_hp_x_r = mix_r;
                        dc_hp_y_r = new_r;
                        mix_r = new_r;
                    }

                    // Denormal prevention: add tiny sub-threshold offset so the CPU
                    // never enters denormal-number slow-path. Completely inaudible.
                    mix_l += 1.0e-24;
                    mix_r += 1.0e-24;

                    // Slew rate limiter: caps the per-sample change to ±SLEW_MAX.
                    // This is a generous last-resort net — legitimate transients
                    // (snare hits, plucks) change by at most ~0.3/sample at 44.1kHz;
                    // runaway spikes are much larger.
                    const SLEW_MAX: f64 = 0.8;
                    mix_l = slew_prev_l + (mix_l - slew_prev_l).clamp(-SLEW_MAX, SLEW_MAX);
                    mix_r = slew_prev_r + (mix_r - slew_prev_r).clamp(-SLEW_MAX, SLEW_MAX);
                    slew_prev_l = mix_l;
                    slew_prev_r = mix_r;

                    // Soft clipper: tanh saturator at drive=1.0.
                    // Below ±0.7 it is perfectly linear; above ±1.0 it curves gently
                    // toward ±1.0 instead of hard-clipping. Completely transparent
                    // at normal mix levels; only activates on occasional hot peaks.
                    mix_l = mix_l.tanh();
                    mix_r = mix_r.tanh();

                    frame_samples_l[frame_idx] = mix_l.clamp(-1.0, 1.0) as f32;
                    frame_samples_r[frame_idx] = mix_r.clamp(-1.0, 1.0) as f32;

                    // Accumulate per-track squared samples for RMS (mono + stereo)
                    for ti in 0..num_tracks {
                        let (tl, tr) = per_track_sample[ti];
                        let vol = smooth_vol.get(ti).copied().unwrap_or(1.0);
                        let sl = tl * vol;
                        let sr = tr * vol;
                        let ts = (sl + sr) * 0.5;
                        track_rms_accum[ti] += (ts * ts) as f32;
                        track_rms_l_accum[ti] += (sl * sl) as f32;
                        track_rms_r_accum[ti] += (sr * sr) as f32;
                    }
                    rms_frame_count += 1;

                    // Advance position
                    pos += beats_per_sample;
                    is_first_frame = false;
                    let _ = frame_idx; // used implicitly via iterator
                }

                // Write position back for UI to read
                pos_cb.store(pos.to_bits(), Ordering::Relaxed);

                // ── Preview sample playback (independent of transport) ──
                // Preview plays even when transport is stopped.
                // Preview is mono — add equally to both channels.
                let mut preview_pos_local = snap.preview_pos;
                if snap.preview_playing && !snap.preview_samples.is_empty() {
                    let preview_ratio = snap.preview_sample_rate as f64 / sample_rate;
                    let preview_end = snap.preview_end_sample; // 0 = play to file end
                    let loop_enabled = snap.preview_loop_enabled;
                    let loop_start = snap.preview_loop_start;
                    for fi in 0..frames {
                        // Check end boundary
                        if preview_end > 0 && preview_pos_local >= preview_end {
                            if loop_enabled {
                                preview_pos_local = loop_start;
                            } else {
                                break;
                            }
                        }
                        let src_pos = preview_pos_local as f64 * preview_ratio;
                        let src_idx = src_pos as usize;
                        if src_idx >= snap.preview_samples.len() {
                            if loop_enabled {
                                preview_pos_local = loop_start;
                                let src_pos2 = preview_pos_local as f64 * preview_ratio;
                                let src_idx2 = src_pos2 as usize;
                                if src_idx2 >= snap.preview_samples.len() {
                                    break;
                                }
                                let s0 = snap.preview_samples[src_idx2];
                                let s1 = if src_idx2 + 1 < snap.preview_samples.len() {
                                    snap.preview_samples[src_idx2 + 1]
                                } else {
                                    s0
                                };
                                let frac = (src_pos2 - src_pos2.floor()) as f32;
                                let interp = s0 + (s1 - s0) * frac;
                                let ps = (interp * snap.master_volume).clamp(-1.0, 1.0);
                                frame_samples_l[fi] = (frame_samples_l[fi] + ps).clamp(-1.0, 1.0);
                                frame_samples_r[fi] = (frame_samples_r[fi] + ps).clamp(-1.0, 1.0);
                                preview_pos_local += 1;
                                continue;
                            } else {
                                break;
                            }
                        }
                        let s0 = snap.preview_samples[src_idx];
                        let s1 = if src_idx + 1 < snap.preview_samples.len() {
                            snap.preview_samples[src_idx + 1]
                        } else {
                            s0
                        };
                        let frac = (src_pos - src_pos.floor()) as f32;
                        let interp = s0 + (s1 - s0) * frac;
                        let preview_sample = interp * snap.master_volume;
                        frame_samples_l[fi] =
                            (frame_samples_l[fi] + preview_sample).clamp(-1.0, 1.0);
                        frame_samples_r[fi] =
                            (frame_samples_r[fi] + preview_sample).clamp(-1.0, 1.0);
                        preview_pos_local += 1;
                    }
                }

                // ── Compute metering and fill oscilloscope ──
                // Write back into shared state (non-blocking; skip if contended)
                if let Ok(mut s) = shared_cb.try_lock() {
                    s.position_beats = pos;
                    // Oscilloscope: write recent mono samples into ring buffer (avoid Vec alloc)
                    let osc_len = s.oscilloscope.len();
                    let mut osc_write = s.osc_write;
                    for (l, r) in frame_samples_l.iter().zip(frame_samples_r.iter()) {
                        let w = osc_write % osc_len;
                        s.oscilloscope[w] = (*l + *r) * 0.5;
                        osc_write += 1;
                    }
                    s.osc_write = osc_write % osc_len;
                    // Per-track RMS from accumulators
                    let n = s.tracks.len();
                    if s.track_rms.len() != n {
                        s.track_rms.resize(n, 0.0);
                    }
                    if s.track_rms_pre_effect.len() != n {
                        s.track_rms_pre_effect.resize(n, 0.0);
                    }
                    if rms_frame_count > 0 {
                        for (i, v) in s.track_rms.iter_mut().enumerate() {
                            let track_rms = if i < track_rms_accum.len() {
                                (track_rms_accum[i] / rms_frame_count as f32).sqrt()
                            } else {
                                0.0
                            };
                            *v = *v * 0.85 + track_rms * 0.15;
                            if *v < 0.0005 {
                                *v = 0.0;
                            }
                        }
                        for (i, v) in s.track_rms_pre_effect.iter_mut().enumerate() {
                            let pre_rms = if i < track_rms_pre_accum.len() {
                                (track_rms_pre_accum[i] / rms_frame_count as f32).sqrt()
                            } else {
                                0.0
                            };
                            *v = *v * 0.85 + pre_rms * 0.15;
                            if *v < 0.0005 {
                                *v = 0.0;
                            }
                        }
                    } else {
                        for v in s.track_rms.iter_mut() {
                            *v *= 0.85; // decay when no frames
                            if *v < 0.0005 {
                                *v = 0.0;
                            }
                        }
                        for v in s.track_rms_pre_effect.iter_mut() {
                            *v *= 0.85;
                            if *v < 0.0005 {
                                *v = 0.0;
                            }
                        }
                    }

                    // Master bus RMS (smoothed)
                    if rms_frame_count > 0 {
                        let mrms = (master_rms_accum / rms_frame_count as f32).sqrt();
                        s.master_rms = s.master_rms * 0.85 + mrms * 0.15;
                        if s.master_rms < 0.0005 {
                            s.master_rms = 0.0;
                        }
                        let mrms_pre = (master_rms_pre_accum / rms_frame_count as f32).sqrt();
                        s.master_rms_pre = s.master_rms_pre * 0.85 + mrms_pre * 0.15;
                        if s.master_rms_pre < 0.0005 {
                            s.master_rms_pre = 0.0;
                        }
                        // Master stereo RMS
                        let ml = (master_rms_l_accum / rms_frame_count as f32).sqrt();
                        s.master_rms_l = s.master_rms_l * 0.85 + ml * 0.15;
                        if s.master_rms_l < 0.0005 { s.master_rms_l = 0.0; }
                        let mr = (master_rms_r_accum / rms_frame_count as f32).sqrt();
                        s.master_rms_r = s.master_rms_r * 0.85 + mr * 0.15;
                        if s.master_rms_r < 0.0005 { s.master_rms_r = 0.0; }
                    } else {
                        s.master_rms *= 0.85;
                        s.master_rms_pre *= 0.85;
                        s.master_rms_l *= 0.85;
                        s.master_rms_r *= 0.85;
                    }

                    // Per-track stereo RMS
                    if s.track_rms_l.len() != n { s.track_rms_l.resize(n, 0.0); }
                    if s.track_rms_r.len() != n { s.track_rms_r.resize(n, 0.0); }
                    if rms_frame_count > 0 {
                        for (i, v) in s.track_rms_l.iter_mut().enumerate() {
                            let r = if i < track_rms_l_accum.len() {
                                (track_rms_l_accum[i] / rms_frame_count as f32).sqrt()
                            } else { 0.0 };
                            *v = *v * 0.85 + r * 0.15;
                            if *v < 0.0005 { *v = 0.0; }
                        }
                        for (i, v) in s.track_rms_r.iter_mut().enumerate() {
                            let r = if i < track_rms_r_accum.len() {
                                (track_rms_r_accum[i] / rms_frame_count as f32).sqrt()
                            } else { 0.0 };
                            *v = *v * 0.85 + r * 0.15;
                            if *v < 0.0005 { *v = 0.0; }
                        }
                    } else {
                        for v in s.track_rms_l.iter_mut() { *v *= 0.85; if *v < 0.0005 { *v = 0.0; } }
                        for v in s.track_rms_r.iter_mut() { *v *= 0.85; if *v < 0.0005 { *v = 0.0; } }
                    }

                    // Per-effect gain reduction for master rack
                    {
                        let n_master = master_effects.len();
                        if s.master_effect_gr.len() != n_master {
                            s.master_effect_gr.resize(n_master, 0.0);
                        }
                        for (fi, (_, fx)) in master_effects.iter().enumerate() {
                            s.master_effect_gr[fi] = fx.gain_reduction_db();
                        }
                    }

                    // Per-effect gain reduction for track effects
                    {
                        let n_tr = track_effects.len();
                        if s.track_effect_gr.len() != n_tr {
                            s.track_effect_gr.resize(n_tr, Vec::new());
                        }
                        for (ti, effects) in track_effects.iter().enumerate() {
                            let n_fx = effects.len();
                            if s.track_effect_gr[ti].len() != n_fx {
                                s.track_effect_gr[ti].resize(n_fx, 0.0);
                            }
                            for (fi, (_, fx)) in effects.iter().enumerate() {
                                s.track_effect_gr[ti][fi] = fx.gain_reduction_db();
                            }
                        }
                    }

                    // Update preview position
                    s.preview_pos = preview_pos_local;
                    if !s.preview_samples.is_empty() && s.preview_playing {
                        let preview_ratio = s.preview_sample_rate as f64 / sample_rate;
                        let src_idx = (preview_pos_local as f64 * preview_ratio) as usize;
                        let past_end =
                            s.preview_end_sample > 0 && preview_pos_local >= s.preview_end_sample;
                        if (src_idx >= s.preview_samples.len() || past_end)
                            && !s.preview_loop_enabled
                        {
                            s.preview_playing = false;
                        }
                    }
                }

                // Write stereo samples to output channels
                // channels >= 2: [L, R, L, R, ...] or [L, R, C, ...] — we only write L/R
                // channels == 1: mono downmix
                for (frame_idx, frame) in data.chunks_mut(channels).enumerate() {
                    let l = if frame_idx < frame_samples_l.len() {
                        frame_samples_l[frame_idx]
                    } else {
                        0.0
                    };
                    let r = if frame_idx < frame_samples_r.len() {
                        frame_samples_r[frame_idx]
                    } else {
                        0.0
                    };
                    if channels >= 2 {
                        frame[0] = l;
                        frame[1] = r;
                        // Fill remaining channels (surround etc.) with silence
                        for ch in frame.iter_mut().skip(2) {
                            *ch = 0.0;
                        }
                    } else {
                        // Mono output: average L+R
                        frame[0] = (l + r) * 0.5;
                    }
                }
            },
            move |err| {
                eprintln!("[audio] Stream error: {}", err);
            },
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[audio] Build stream error: {}", e);
                return;
            }
        };

        if let Err(e) = stream.play() {
            eprintln!("[audio] Play error: {}", e);
            return;
        }

        // Keep thread alive — stream is dropped when this thread exits
        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    });

    Ok((shared, pos_atomic))
}
