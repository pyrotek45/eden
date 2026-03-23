# Automation Parameter Manager — Design Document

## Overview

The **Automation Parameter Manager** allows a single automation clip to control one or more
rack parameters across any tracks in the project. This replaces the current one-clip-one-implicit-
target model with an explicit, flexible binding system.

---

## Motivation

Currently, automation clips live on an "Automation" track but have no formal connection to
the parameter they control — the binding is implied by track ordering and is fragile. Users
cannot:

- Control multiple parameters with a single automation curve.
- Easily see *what* a given automation clip controls.
- Re-target an automation clip to a different parameter without recreating it.

---

## Data Model Changes

### 1. `AutomationTarget` (already exists in `models.rs`)

```rust
pub struct AutomationTarget {
    pub track_id: u32,   // Which track the rack belongs to
    pub slot_id: u32,    // Which rack slot (RackSlot.slot_id)
    pub param_id: String, // Which parameter within that slot (RackParam.id)
}
```

### 2. Extend `AutoClip`

Add an optional list of controlled parameters to `AutoClip`:

```rust
pub struct AutoClip {
    pub name: String,
    pub start_time: f64,
    pub length: f64,
    pub points: Vec<AutoPoint>,

    // NEW: explicit list of parameters this clip drives.
    // If empty, falls back to legacy behaviour (track implicit binding).
    pub targets: Vec<AutomationTarget>,
}
```

The normalised `AutoPoint.value` (0.0–1.0) is mapped to each target's `[min, max]` range.

---

## UI — Automation Clip Editor (bottom panel)

### Current layout

```
┌──────────────────────────────────────────────────┐
│  [Clip name]          [zoom]  [loop]  [grid]     │
│──────────────────────────────────────────────────│
│  time →                                          │
│  Automation curve editor (points, lines)         │
└──────────────────────────────────────────────────┘
```

### New layout

```
┌──────────────────────────────────────────────────┐
│  [Clip name]          [zoom]  [loop]  [grid]     │
│────────────────┬─────────────────────────────────│
│ PARAMETERS     │  time →                         │
│ ┌────────────┐ │  Automation curve editor        │
│ │ Track 1    │ │  (unchanged)                    │
│ │ · LP Filt  │ │                                 │
│ │   Cutoff ✓ │ │                                 │
│ │ Track 2    │ │                                 │
│ │ · Delay    │ │                                 │
│ │   Wet   ✓  │ │                                 │
│ └────────────┘ │                                 │
│ [+ Add Param]  │                                 │
└────────────────┴─────────────────────────────────┘
```

The left sidebar (width ≈ 180 px) lists all currently bound `AutomationTarget`s. Each entry:

- Shows track name → slot name → param name
- Has a **×** remove button
- Clicking the entry highlights the corresponding knob in the rack (existing
  `rack_highlight_param` mechanism)

**[+ Add Param]** opens a hierarchical picker:

```
Select Parameter
────────────────
▶ MIDI Track 1
   ▶ LP Filter
      · Cutoff        ← click to bind
      · Resonance
   ▶ Delay
      · Wet
      · Time
▶ MIDI Track 2
   ...
```

The picker lists every enabled rack slot on every non-automation track, and every parameter
within each slot.

---

## Audio Engine Changes

In `audio.rs`, the callback already applies automation via `AutomationTarget` lookups.
The change is minimal: instead of looking up one implicit target per clip, iterate
`clip.targets` and apply the same normalised value to each bound parameter.

```rust
// In the audio callback, for each playing AutoClip:
let norm_value = interpolate_points(&clip.points, beat_in_clip);

for target in &clip.targets {
    // Find matching track → slot → param and set value
    if let Some(track) = audio_tracks.iter_mut().find(|t| t.id == target.track_id) {
        if let Some(slot) = track.rack.iter_mut().find(|s| s.slot_id == target.slot_id) {
            if let Some(param) = slot.params.iter_mut().find(|p| p.id == target.param_id) {
                let range = param.max - param.min;
                param.value = param.min + norm_value as f32 * range;
            }
        }
    }
}
```

> **Note:** The audio thread currently does NOT own mutable rack params — they are snapshotted
> from the main thread each frame. The correct approach is to **write automation-driven values
> into the snapshot** in `main.rs` before building `AudioTrack`, rather than in the audio
> callback. This avoids data races.

---

## Implementation Plan

### Phase 1 — Data model
1. Add `pub targets: Vec<AutomationTarget>` to `AutoClip` (with `#[serde(default)]`).
2. Add a migration: when loading old projects, if `targets` is empty, attempt to infer the
   target from the automation track's position relative to MIDI tracks (existing behaviour).

### Phase 2 — Automation application in `main.rs`
3. Before building `AudioTrack` snapshots, evaluate all playing automation clips:
   - Compute `norm_value` from `clip.points` at current playhead beat.
   - For each `target` in `clip.targets`, write the mapped value into
     `project.tracks[ti].rack[si].params[pi].value`.
   - This drives the audio through the existing snapshot mechanism — no audio-thread changes.

### Phase 3 — UI: parameter list sidebar
4. In `draw_bottom_panel` → automation editor branch, split the editor area into
   a left sidebar (parameter list) and right curve editor.
5. Draw each `AutomationTarget` as a list row with remove button.
6. Implement `[+ Add Param]` button that sets `state.auto_param_picker_open = true`.

### Phase 4 — UI: parameter picker overlay
7. Draw the hierarchical picker as a modal overlay (similar to the existing file browser modal).
8. On selection, push an `AddAutoTarget { clip_track_id, clip_idx, target }` command.

### Phase 5 — Rack integration
9. Right-clicking a rack param knob shows a context menu option **"Automate this param"**
   that opens an automation clip selector or creates a new automation clip pre-bound to this
   parameter. This is optional / stretch goal.

---

## Commands (Undo/Redo)

```rust
/// Bind an AutomationTarget to an AutoClip.
pub struct AddAutoTarget {
    pub track_id: u32,
    pub clip_idx: usize,
    pub target: AutomationTarget,
}

/// Remove an AutomationTarget from an AutoClip.
pub struct RemoveAutoTarget {
    pub track_id: u32,
    pub clip_idx: usize,
    pub target_idx: usize,
    pub removed: Option<AutomationTarget>,
}
```

---

## State Fields Required

```rust
// In AppState:
pub auto_param_picker_open: bool,
pub auto_param_picker_track_idx: Option<usize>,  // expanded track in picker
pub auto_param_picker_slot_idx: Option<usize>,   // expanded slot in picker
```

---

## Compatibility

- Old `AutoClip`s with `targets: []` continue to work via legacy implicit binding until the
  user explicitly adds targets.
- The `AutomationTarget` struct already exists and is serializable — no breaking schema change.
