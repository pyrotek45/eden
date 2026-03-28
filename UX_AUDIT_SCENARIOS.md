# Eden DAW — User Scenario Audit

> **Purpose:** Five realistic personas exercise every workflow end-to-end.
> Each scenario lists: (1) what the user does, (2) what works well today,
> (3) friction points / bugs, and (4) suggested improvements.
>
> **These files are for review only — do NOT commit.**

---

## Persona 1 — "Alex the Bedroom Producer" (Electronic / Lo-Fi)

**Profile:** Makes beats and lo-fi chillhop. Uses soft synths, samples,
simple arrangement. Exports stems and a master WAV.

### Workflow

1. **Create project** → File ▸ New, set BPM to 85.
2. **Add 3 MIDI tracks:** Drums (Sampler), Pad (HyperSaw), Bass (Analog).
3. **Browse samples** via the left panel file browser → load a kick into Sampler.
4. **Draw MIDI notes** in the piano roll for each track.
5. **Add effects:** Reverb on Pad, Compressor + Limiter on master.
6. **Adjust mix:** set volumes, pans, CStrip2 EQ per track.
7. **Automate filter cutoff** on the pad over 8 bars.
8. **Solo/mute** tracks to audition.
9. **Render** to WAV, 44100 Hz.

### What Works Well

| Feature | Notes |
|---------|-------|
| Synth variety | Analog, HyperSaw, Monolith, Sampler cover the bases |
| MIDI piano roll | Responsive note drawing, velocity editing |
| Effect chain | Full per-track + master rack with bypass toggle |
| CStrip2 per track | Quick tonal shaping without extra rack slots |
| Keyboard piano (QWERTY) | Nice for quick auditioning |
| Save/Load JSON | Lossless round-trip, every knob preserved |
| Undo/Redo | Snapshot-based, every operation undoable |

### Friction Points

| Issue | Severity | Details |
|-------|----------|---------|
| **No sample preview in browser** | Medium | You can browse .wav files but there's no way to audition them before loading. |
| **No drag-and-drop for samples** | Medium | Must browse manually; can't drag from desktop. |
| **No copy/paste clips** | High | No Ctrl+C/V for clips in arrangement. Must duplicate note-by-note. |
| **No BPM tap-tempo** | Low | Must type BPM manually. |
| **No clip color picker** | Low | Colors are assigned automatically; no way to customize per-clip. |
| **No MIDI input routing** | Medium | QWERTY keyboard works, but no external MIDI controller input for recording. |
| **No audio recording** | High | Can't record mic/line-in; audio tracks require pre-made WAV files. |
| **Export only WAV** | Medium | No FLAC/MP3/OGG export option. |

### Suggested Improvements

1. **Sample audition button** in the file browser — click to preview before committing.
2. **Clip clipboard** — Ctrl+C/V for arrangement clips (MIDI, Audio, Automation).
3. **Audio recording** — record from system input into an audio track.
4. **MIDI controller input** — route external MIDI to instruments for live play/recording.

---

## Persona 2 — "Sam the Sound Designer" (Ambient / Textural)

**Profile:** Layers pads, drones, textures. Relies heavily on long reverbs,
heavy processing, and automation. Projects have 10–20 tracks.

### Workflow

1. **Create 12 MIDI tracks** with various synths.
2. **Stack effects:** Each track has 3-5 effects (filter → chorus → reverb → compressor → EQ).
3. **Automate** multiple parameters: filter cutoff, reverb decay, chorus depth.
4. **Use sidechain compression** on a pad ducked by a rhythmic pulse.
5. **Set up long notes** (8–16 bars) and let reverb/delay tails ring.
6. **Fine-tune CStrip2** treble/bass per track for spectral balance.
7. **Master:** Compressor → EQ → Limiter chain.
8. **Render** at 48000 Hz for high quality.

### What Works Well

| Feature | Notes |
|---------|-------|
| Automation curves | Linear interpolation, target any rack param |
| Sidechain compression | Per-effect sidechain routing to any track |
| Multiple sample rates | 44100/48000 supported in render |
| Reverb quality | Hall-style reverb with diffuse/modulate/size/decay controls |
| Effect chain order | Effects process in rack order — signal chain is predictable |
| Tail rendering | `extra_secs` in render captures reverb/delay tails |

### Friction Points

| Issue | Severity | Details |
|-------|----------|---------|
| **Only linear automation** | Medium | No curves (exponential, S-curve, step). Linear only. |
| **No freeze/bounce** | Medium | Heavy projects with 12 tracks of effects may struggle on older hardware. |
| **No send/return routing** | High | Can't share one reverb instance across tracks. Each track needs its own. |
| **No group buses** | High | Can't route multiple tracks through a shared bus with shared processing. |
| **Automation clip must target one param** | Low | Need separate clips for each automated param. |
| **No LFO or modulation source** | Medium | Manual automation only; no auto-generated modulation. |
| **No effect preset system** | Medium | Every effect chain must be built from scratch each time. |

### Suggested Improvements

1. **Send/return buses** — shared reverb/delay to reduce CPU and memory.
2. **Automation curve types** — at minimum: exponential and step modes.
3. **Effect presets** — save/recall per-effect or per-chain settings.
4. **Track freeze** — render a track offline and mute the live processing.

---

## Persona 3 — "Jordan the Songwriter" (Singer-Songwriter / Pop)

**Profile:** Writes songs with vocals + guitar (audio) and synth backing
(MIDI). Needs audio recording, basic editing, and a polished mix.

### Workflow

1. **Import guitar recording** as an audio clip.
2. **Create MIDI tracks** for piano (Analog) and strings (HyperSaw).
3. **Draw chord progressions** in the piano roll.
4. **Time-stretch** the audio clip to match BPM.
5. **Add EQ and compression** on the guitar.
6. **Fade in/out** on audio clips.
7. **Master with limiter** at -0.3 dBFS ceiling.
8. **Export** WAV for distribution.

### What Works Well

| Feature | Notes |
|---------|-------|
| Audio clips | Import, trim, gain adjust, fade in/out |
| MIDI + Audio coexistence | Both track types in same project |
| EQ 3-band | Useful for basic guitar cleanup |
| Limiter | Brick-wall with ceiling control |
| Fade in/out | Per-clip fade controls with smooth curves |
| File browser | Navigate to audio files easily |

### Friction Points

| Issue | Severity | Details |
|-------|----------|---------|
| **No time-stretch** | High | Can't change audio clip tempo/pitch independently. |
| **No audio recording** | Critical | Can't record vocals or guitar — must use external app. |
| **No metronome** | High | No click track for recording or playback. |
| **No pitch correction** | Low | Expected at this point, but not present. |
| **No marker/section system** | Medium | Can't label verse/chorus/bridge sections. |
| **No track input monitoring** | Medium | Can't hear live input while recording. |
| **Audio clip has no waveform zoom** | Low | Waveform display exists but no vertical zoom. |

### Suggested Improvements

1. **Audio recording** — record from system input with monitoring.
2. **Metronome** — configurable click track (accent on beat 1, volume, sound).
3. **Markers/regions** — label arrangement sections for navigation.
4. **Time-stretch** — basic pitch-independent time-stretch for audio clips.

---

## Persona 4 — "Riley the Remix Artist" (EDM / Dance)

**Profile:** Takes existing stems, chops them, layers synths, builds drops.
Uses lots of automation, sidechain pumping, and tempo changes.

### Workflow

1. **Import 6-8 audio stems** (vocals, drums, bass, synths).
2. **Chop audio clips** — split, trim, move.
3. **Layer MIDI synths** on top — lead, sub-bass, arps.
4. **Set up arpeggiator** MIDI effect with chord stacking.
5. **Automate autoduck** for sidechain pump effect.
6. **Create tempo automation** for buildups/breakdowns.
7. **Master bus:** Compressor → EQ → Limiter with tight ceiling.
8. **Loop section** for detail work.
9. **Render** at 44100 Hz.

### What Works Well

| Feature | Notes |
|---------|-------|
| Arpeggiator | Rate, octaves, pattern (up/down/updown/random), gate length |
| Chord MIDI effect | Major/minor/7th/sus4/dim with voicing options |
| Autoduck | Tempo-synced volume ducking with curve, shift, period |
| Loop region | Set loop start/end for focused editing |
| Audio clip chopping | Split, trim, move clips on timeline |
| Multiple time signatures | 4/4 and others supported |

### Friction Points

| Issue | Severity | Details |
|-------|----------|---------|
| **No tempo automation** | High | BPM is project-wide; can't automate tempo changes for builds. |
| **No audio clip slice/chop tool** | Medium | Must manually split clips; no beat-slice mode. |
| **No crossfade between clips** | Medium | When two audio clips meet, there's a hard cut. |
| **No warp markers** | High | Can't align audio to grid at specific points. |
| **No sidechain to audio tracks** | Low | Sidechain compression works, but only for MIDI track signals. |
| **No grid snap options for audio** | Low | Audio clips don't snap to beat grid as precisely. |
| **No duplicate track** | Medium | Can't clone a track with all its settings and clips. |

### Suggested Improvements

1. **Tempo automation** — allow BPM changes over time with smooth ramps.
2. **Crossfade** — automatic or manual crossfade when clips overlap.
3. **Duplicate track** — clone track with all rack, clips, and settings.
4. **Beat-slice mode** — automatically chop audio at transients/beats.

---

## Persona 5 — "Casey the Educator" (Teaching / Demos)

**Profile:** Uses Eden for teaching music production concepts. Creates
simple demos, needs clear UI, and frequently saves/loads example projects.

### Workflow

1. **Open demo project** (`Project::demo()` preset).
2. **Walk through instrument types** — switch between Analog, HyperSaw, Sampler.
3. **Demonstrate effect chain** — add effects one by one, A/B compare.
4. **Show automation** — draw automation clip, play to hear param change.
5. **Explain mixing** — adjust volumes, pans, solo/mute.
6. **Save project** for students to load later.
7. **Toggle themes** to find one that projects well.
8. **Use keyboard shortcuts** for fast workflow demo.

### What Works Well

| Feature | Notes |
|---------|-------|
| Theme system | Multiple themes, easy switching (next theme hotkey) |
| Help screen (F1) | Shows keyboard shortcuts |
| Demo project | Pre-built demo for quick start |
| Compact JSON save format | Easy to share project files |
| CStrip2 bypass toggle | A/B comparison built-in |
| Effect enable/disable | Quick per-slot bypass |
| Clean UI layout | Arrangement / Mixer / Edit modes |

### Friction Points

| Issue | Severity | Details |
|-------|----------|---------|
| **No tooltip system** | Medium | Hovering over a knob doesn't explain what it does. |
| **No preset browser** | Medium | Can't save/load synth presets — every sound starts from scratch. |
| **No project templates** | Low | Only one demo project; no genre templates. |
| **Limited documentation** | Medium | Help screen shows shortcuts but not workflow guidance. |
| **No undo history panel** | Low | Can undo, but can't see the history stack visually. |
| **Piano roll scale highlighting** | Low | No option to highlight scale notes (C major, etc.). |
| **No MIDI learn** | Low | Can't map MIDI CC to parameters for live demos. |

### Suggested Improvements

1. **Tooltip on hover** — show param name, current value, and range.
2. **Preset system** — save/load per-instrument and per-effect presets.
3. **Undo history panel** — visual list of recent actions.
4. **Scale highlighting** in piano roll — shade notes outside the key.

---

## Cross-Cutting Improvement Priorities

Ranked by impact across all personas:

| Priority | Feature | Personas Affected |
|----------|---------|-------------------|
| 1 | **Audio recording** | Alex, Jordan, Riley |
| 2 | **Clip copy/paste** | Alex, Riley, Casey |
| 3 | **Send/return buses** | Sam, Riley |
| 4 | **Metronome** | Jordan, Riley, Casey |
| 5 | **Automation curves** | Sam, Riley |
| 6 | **Preset system** | Sam, Casey |
| 7 | **Tempo automation** | Riley |
| 8 | **Tooltip system** | Casey, Alex |
| 9 | **Sample preview** | Alex, Jordan |
| 10 | **Crossfade** | Riley, Jordan |
