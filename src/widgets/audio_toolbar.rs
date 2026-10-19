//! Measured audio-editor controls, shared by drawing, hit targets and popup overlays.
use sdl2::rect::Rect;

#[derive(Clone, Copy)]
pub enum AudioControl {
    Rewind,
    Stop,
    Play,
    Loop,
    Unique,
    Select,
    Normalize,
    Trim,
    Fit,
    Cut,
    Paste,
    Snap,
    Grid,
    Gain,
    FadeIn,
    FadeOut,
    Export,
    Effect,
    Apply,
}

const AUDIO_CONTROL_COUNT: usize = AudioControl::Apply as usize + 1;

pub struct AudioToolbarLayout {
    slots: [Rect; AUDIO_CONTROL_COUNT],
    pub info: Rect,
    pub height: i32,
    pub label_advance: i32,
}

impl AudioToolbarLayout {
    pub fn control(&self, control: AudioControl) -> Rect {
        self.slots[control as usize]
    }
}

/// Wrap complete controls; never shrink text or let the right-hand FX overlap tools.
pub fn audio_toolbar_layout(width: i32, font_scale: i32, shared: bool) -> AudioToolbarLayout {
    use AudioControl::*;
    const SIDE_PADDING: i32 = 6;
    const GAP: i32 = 4;
    // Labels use a one-pixel gap; button captions use a scale-dependent gap.
    let advance = (4 * font_scale + 1).max(4 * font_scale + font_scale / 2);
    let control_height = (5 * font_scale + 8).max(20);
    let text_width = |label: &str| label.chars().count() as i32 * advance + 16;
    let mut slots = [Rect::new(0, 0, 0, 0); AUDIO_CONTROL_COUNT];
    let mut x = SIDE_PADDING;
    let mut y = GAP;
    let mut place = |item_width: i32| {
        if x + item_width > width - SIDE_PADDING {
            x = SIDE_PADDING;
            y += control_height + GAP;
        }
        let rect = Rect::new(x, y, item_width as u32, control_height as u32);
        x += item_width + GAP;
        rect
    };
    for c in [Rewind, Stop, Play, Loop] {
        slots[c as usize] = place(control_height.max(22));
    }
    if shared {
        slots[Unique as usize] = place(text_width("UNIQUE"));
    }
    for (c, label) in [
        (Select, "SEL"),
        (Normalize, "NORM"),
        (Trim, "TRIM"),
        (Fit, "FIT"),
        (Cut, "CUT"),
        (Paste, "PASTE"),
    ] {
        slots[c as usize] = place(text_width(label).max(40));
    }
    slots[Snap as usize] = place(control_height.max(22));
    slots[Grid as usize] = place(5 * advance + 26);
    slots[Gain as usize] = place(4 * advance + 6 + 70);
    slots[FadeIn as usize] = place(3 * advance + 6 + 50);
    slots[FadeOut as usize] = place(4 * advance + 6 + 50);
    slots[Export as usize] = place(text_width("Export"));
    let effect_width = "Gain -6dB".chars().count() as i32 * advance + 26;
    let apply_width = text_width("APPLY");
    let pair = place(effect_width + GAP + apply_width);
    slots[Effect as usize] = Rect::new(pair.x(), pair.y(), effect_width as u32, pair.height());
    slots[Apply as usize] = Rect::new(
        pair.x() + effect_width + GAP,
        pair.y(),
        apply_width as u32,
        pair.height(),
    );
    let info = Rect::new(
        SIDE_PADDING,
        pair.bottom() + GAP,
        (width - 2 * SIDE_PADDING).max(0) as u32,
        (5 * font_scale + 8) as u32,
    );
    AudioToolbarLayout {
        slots,
        height: info.bottom() + 2,
        info,
        label_advance: advance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_tool_fits_without_overlaps_at_supported_widths_and_fonts() {
        for width in [520, 640, 800, 1280, 1440] {
            for font in 1..=4 {
                for shared in [false, true] {
                    let layout = audio_toolbar_layout(width, font, shared);
                    let controls: Vec<_> = layout
                        .slots
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| *i != AudioControl::Unique as usize || shared)
                        .map(|(_, r)| r)
                        .collect();
                    for (i, r) in controls.iter().enumerate() {
                        assert!(
                            r.x() >= 6 && r.right() <= width - 6,
                            "control {i} at {width}px / font {font}"
                        );
                        assert!(r.bottom() <= layout.info.y());
                        for other in controls.iter().skip(i + 1) {
                            assert!(!r.has_intersection(**other));
                        }
                    }
                    assert!(
                        layout.height <= 184,
                        "leave room for audio at minimum editor height"
                    );
                    let fx = layout.control(AudioControl::Effect);
                    let apply = layout.control(AudioControl::Apply);
                    assert_eq!(fx.y(), apply.y());
                    assert!(fx.right() < apply.x());
                    assert_eq!(layout.control(AudioControl::Rewind).y(), 4);
                }
            }
        }
    }
}
