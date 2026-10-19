// Eden DAW — Clip handle helpers

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::app::input::InputState;
use crate::theme::Theme;

pub const CLIP_HANDLE_W: i32 = 6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ClipHitZone {
    None,
    LeftHandle,
    RightHandle,
    /// Top strip (double-click to rename); selection/drag matches the body.
    Header,
    Body,
}

pub fn clip_hit_test(
    input: &InputState,
    clip_x: i32,
    clip_y: i32,
    clip_w: i32,
    clip_h: i32,
    header_h: i32,
) -> ClipHitZone {
    if !input.mouse_in_rect(clip_x, clip_y, clip_w.max(4), clip_h) {
        return ClipHitZone::None;
    }
    let lx = input.mouse_x - clip_x;
    let ly = input.mouse_y - clip_y;
    let handle_w = (clip_w / 3).clamp(1, CLIP_HANDLE_W);
    if clip_w >= 4 {
        if lx < handle_w {
            return ClipHitZone::LeftHandle;
        }
        if lx >= clip_w - handle_w {
            return ClipHitZone::RightHandle;
        }
    }
    if ly < header_h {
        return ClipHitZone::Header;
    }
    ClipHitZone::Body
}

#[allow(clippy::too_many_arguments)]
pub fn draw_clip_handles(
    canvas: &mut Canvas<Window>,
    theme: &Theme,
    cx: i32,
    cy: i32,
    cw: i32,
    ch: i32,
    hover_zone: ClipHitZone,
    selected: bool,
) {
    let hovered = hover_zone != ClipHitZone::None;
    if !hovered && !selected {
        return;
    }
    if cw < 4 {
        return;
    }
    let handle_w = (cw / 3).clamp(1, CLIP_HANDLE_W);

    let base_hc = if selected {
        Theme::c(theme.accent)
    } else {
        sdl2::pixels::Color::RGBA(255, 255, 255, 60)
    };
    let hot_hc = sdl2::pixels::Color::RGBA(255, 255, 255, 200);

    let left_hc = if hover_zone == ClipHitZone::LeftHandle {
        hot_hc
    } else {
        base_hc
    };
    let right_hc = if hover_zone == ClipHitZone::RightHandle {
        hot_hc
    } else {
        base_hc
    };

    canvas.set_draw_color(left_hc);
    let _ = canvas.fill_rect(Rect::new(cx, cy, handle_w as u32, ch as u32));
    canvas.set_draw_color(right_hc);
    let _ = canvas.fill_rect(Rect::new(
        cx + cw - handle_w,
        cy,
        handle_w as u32,
        ch as u32,
    ));

    let gc = sdl2::pixels::Color::RGBA(0, 0, 0, 90);
    canvas.set_draw_color(gc);
    for i in 0..3 {
        let gy = cy + ch / 2 - 4 + i * 4;
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(cx + 1, gy),
            sdl2::rect::Point::new(cx + handle_w - 1, gy),
        );
        let _ = canvas.draw_line(
            sdl2::rect::Point::new(cx + cw - handle_w + 1, gy),
            sdl2::rect::Point::new(cx + cw - 1, gy),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_clips_keep_distinct_trim_handles_and_a_draggable_body() {
        for width in [4, 9, 18, 24, 100] {
            let mut i = InputState::default();
            i.mouse_y = 20;
            i.mouse_x = 0;
            assert_eq!(
                clip_hit_test(&i, 0, 0, width, 40, 10),
                ClipHitZone::LeftHandle
            );
            i.mouse_x = width - 1;
            assert_eq!(
                clip_hit_test(&i, 0, 0, width, 40, 10),
                ClipHitZone::RightHandle
            );
            i.mouse_x = width / 2;
            assert_eq!(clip_hit_test(&i, 0, 0, width, 40, 10), ClipHitZone::Body);
        }
    }
}
