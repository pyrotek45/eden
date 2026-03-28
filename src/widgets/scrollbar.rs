// Eden DAW — Scrollbar + squeeze-handle scrollbar

use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use crate::input::{InputState, WidgetId};
use crate::theme::Theme;

#[derive(Clone, Copy, PartialEq)]
pub enum ScrollbarDir {
    Horizontal,
    Vertical,
}

#[allow(clippy::too_many_arguments)]
pub fn scrollbar(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: WidgetId,
    x: i32,
    y: i32,
    length: i32,
    thickness: i32,
    dir: ScrollbarDir,
    scroll: f32,
    thumb_ratio: f32,
) -> f32 {
    if length <= 0 || thickness <= 0 {
        return scroll;
    }
    let thumb_ratio = thumb_ratio.clamp(0.05, 1.0);
    let thumb_len = ((length as f32 * thumb_ratio) as i32).max(12);
    let travel = (length - thumb_len).max(1);
    let thumb_offset = (scroll * travel as f32) as i32;

    let (track_rect, thumb_rect) = match dir {
        ScrollbarDir::Horizontal => (
            Rect::new(x, y, length as u32, thickness as u32),
            Rect::new(
                x + thumb_offset,
                y + 1,
                thumb_len as u32,
                (thickness - 2).max(1) as u32,
            ),
        ),
        ScrollbarDir::Vertical => (
            Rect::new(x, y, thickness as u32, length as u32),
            Rect::new(
                x + 1,
                y + thumb_offset,
                (thickness - 2).max(1) as u32,
                thumb_len as u32,
            ),
        ),
    };

    // Draw track
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(20, 20, 24, 200));
    let _ = canvas.fill_rect(track_rect);
    canvas.set_draw_color(Theme::c(theme.panel_border));
    let _ = canvas.draw_rect(track_rect);

    let thumb_hover = input.mouse_in_rect(
        thumb_rect.x(),
        thumb_rect.y(),
        thumb_rect.width() as i32,
        thumb_rect.height() as i32,
    );
    let track_hover = input.mouse_in_rect(
        track_rect.x(),
        track_rect.y(),
        track_rect.width() as i32,
        track_rect.height() as i32,
    );

    if thumb_hover && input.mouse_pressed && !input.consumed {
        input.drag_widget = id;
        input.active_widget = id;
        input.drag_start_value = scroll as f64;
        input.drag_start_x = input.mouse_x;
        input.drag_start_y = input.mouse_y;
        input.consume();
    }

    if track_hover && !thumb_hover && input.mouse_pressed && !input.consumed {
        let raw = match dir {
            ScrollbarDir::Horizontal => (input.mouse_x - x - thumb_len / 2) as f32 / travel as f32,
            ScrollbarDir::Vertical => (input.mouse_y - y - thumb_len / 2) as f32 / travel as f32,
        };
        input.drag_widget = id;
        input.active_widget = id;
        input.drag_start_value = raw.clamp(0.0, 1.0) as f64;
        input.drag_start_x = input.mouse_x;
        input.drag_start_y = input.mouse_y;
        input.consume();
        return raw.clamp(0.0, 1.0);
    }

    let mut new_scroll = scroll;
    if input.drag_widget == id && input.mouse_down {
        let delta_px = match dir {
            ScrollbarDir::Horizontal => (input.mouse_x - input.drag_start_x) as f32,
            ScrollbarDir::Vertical => (input.mouse_y - input.drag_start_y) as f32,
        };
        let raw = input.drag_start_value as f32 + delta_px / travel as f32;
        new_scroll = raw.clamp(0.0, 1.0);
    }

    // Draw thumb
    let thumb_col = if input.drag_widget == id {
        Theme::c(theme.accent)
    } else if thumb_hover {
        Theme::c(theme.accent_hover)
    } else {
        Theme::c(theme.slider_thumb)
    };
    canvas.set_draw_color(thumb_col);
    let _ = canvas.fill_rect(thumb_rect);

    new_scroll
}

/// Like `scrollbar` but also draws squeeze handles at the left/right (or top/bottom)
/// edges of the thumb. Dragging a handle changes the `thumb_ratio` (zooming in/out).
/// Returns `(new_scroll, new_thumb_ratio)`.
#[allow(clippy::too_many_arguments)]
pub fn scrollbar_with_squeeze(
    canvas: &mut Canvas<Window>,
    input: &mut InputState,
    theme: &Theme,
    id: WidgetId,
    squeeze_id_lo: WidgetId,
    squeeze_id_hi: WidgetId,
    x: i32,
    y: i32,
    length: i32,
    thickness: i32,
    dir: ScrollbarDir,
    scroll: f32,
    thumb_ratio: f32,
) -> (f32, f32) {
    let thumb_ratio_clamped = thumb_ratio.clamp(0.05, 1.0);
    const HANDLE_SIZE: i32 = 14;
    const HIT_PAD: i32 = 4;
    let thumb_len = ((length as f32 * thumb_ratio_clamped) as i32).max(HANDLE_SIZE * 2 + 2);
    let travel = (length - thumb_len).max(1);
    let thumb_offset = (scroll.clamp(0.0, 1.0) * travel as f32) as i32;

    let (lo_rect, hi_rect) = match dir {
        ScrollbarDir::Horizontal => (
            Rect::new(x + thumb_offset, y, HANDLE_SIZE as u32, thickness as u32),
            Rect::new(
                x + thumb_offset + thumb_len - HANDLE_SIZE,
                y,
                HANDLE_SIZE as u32,
                thickness as u32,
            ),
        ),
        ScrollbarDir::Vertical => (
            Rect::new(x, y + thumb_offset, thickness as u32, HANDLE_SIZE as u32),
            Rect::new(
                x,
                y + thumb_offset + thumb_len - HANDLE_SIZE,
                thickness as u32,
                HANDLE_SIZE as u32,
            ),
        ),
    };

    let lo_hit = match dir {
        ScrollbarDir::Horizontal => Rect::new(
            lo_rect.x() - HIT_PAD,
            lo_rect.y() - HIT_PAD,
            (lo_rect.width() as i32 + HIT_PAD * 2) as u32,
            (lo_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
        ScrollbarDir::Vertical => Rect::new(
            lo_rect.x() - HIT_PAD,
            lo_rect.y() - HIT_PAD,
            (lo_rect.width() as i32 + HIT_PAD * 2) as u32,
            (lo_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
    };
    let hi_hit = match dir {
        ScrollbarDir::Horizontal => Rect::new(
            hi_rect.x() - HIT_PAD,
            hi_rect.y() - HIT_PAD,
            (hi_rect.width() as i32 + HIT_PAD * 2) as u32,
            (hi_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
        ScrollbarDir::Vertical => Rect::new(
            hi_rect.x() - HIT_PAD,
            hi_rect.y() - HIT_PAD,
            (hi_rect.width() as i32 + HIT_PAD * 2) as u32,
            (hi_rect.height() as i32 + HIT_PAD * 2) as u32,
        ),
    };

    let lo_hover = input.mouse_in_rect(
        lo_hit.x(),
        lo_hit.y(),
        lo_hit.width() as i32,
        lo_hit.height() as i32,
    );
    let hi_hover = input.mouse_in_rect(
        hi_hit.x(),
        hi_hit.y(),
        hi_hit.width() as i32,
        hi_hit.height() as i32,
    );

    if input.mouse_pressed && lo_hover && input.drag_widget == WidgetId::None {
        input.drag_widget = squeeze_id_lo;
        input.active_widget = squeeze_id_lo;
        input.drag_start_value = thumb_ratio as f64;
        input.drag_start_value2 = scroll as f64;
        input.drag_start_x = match dir {
            ScrollbarDir::Horizontal => input.mouse_x,
            ScrollbarDir::Vertical => input.mouse_y,
        };
        input.consumed = true;
    } else if input.mouse_pressed && hi_hover && input.drag_widget == WidgetId::None {
        input.drag_widget = squeeze_id_hi;
        input.active_widget = squeeze_id_hi;
        input.drag_start_value = thumb_ratio as f64;
        input.drag_start_value2 = scroll as f64;
        input.drag_start_x = match dir {
            ScrollbarDir::Horizontal => input.mouse_x,
            ScrollbarDir::Vertical => input.mouse_y,
        };
        input.consumed = true;
    }

    let new_scroll = if input.active_widget == squeeze_id_lo
        || input.active_widget == squeeze_id_hi
        || input.drag_widget == squeeze_id_lo
        || input.drag_widget == squeeze_id_hi
    {
        scroll
    } else {
        scrollbar(
            canvas,
            input,
            theme,
            id,
            x,
            y,
            length,
            thickness,
            dir,
            scroll,
            thumb_ratio,
        )
    };

    if input.active_widget == squeeze_id_lo
        || input.active_widget == squeeze_id_hi
        || input.drag_widget == squeeze_id_lo
        || input.drag_widget == squeeze_id_hi
    {
        canvas.set_draw_color(Theme::c(theme.scrollbar_bg));
        let _ = match dir {
            ScrollbarDir::Horizontal => {
                canvas.fill_rect(Rect::new(x, y, length as u32, thickness as u32))
            }
            ScrollbarDir::Vertical => {
                canvas.fill_rect(Rect::new(x, y, thickness as u32, length as u32))
            }
        };
        canvas.set_draw_color(Theme::c(theme.scrollbar_thumb));
        let _ = match dir {
            ScrollbarDir::Horizontal => canvas.fill_rect(Rect::new(
                x + thumb_offset,
                y + 1,
                thumb_len as u32,
                (thickness - 2).max(1) as u32,
            )),
            ScrollbarDir::Vertical => canvas.fill_rect(Rect::new(
                x + 1,
                y + thumb_offset,
                (thickness - 2).max(1) as u32,
                thumb_len as u32,
            )),
        };
    }

    let mut new_thumb_ratio = thumb_ratio;
    let mut new_scroll = new_scroll;
    if input.mouse_down
        && (input.drag_widget == squeeze_id_lo || input.drag_widget == squeeze_id_hi)
    {
        let cur_pos = match dir {
            ScrollbarDir::Horizontal => input.mouse_x,
            ScrollbarDir::Vertical => input.mouse_y,
        };
        let drag_delta_px = (cur_pos - input.drag_start_x) as f64;
        let ratio_delta = (drag_delta_px / length as f64) as f32;

        let orig_ratio = (input.drag_start_value as f32).clamp(0.05, 1.0);
        let orig_scroll = (input.drag_start_value2 as f32).clamp(0.0, 1.0);
        let old_left = orig_scroll * (1.0 - orig_ratio);
        let old_right = old_left + orig_ratio;

        if input.drag_widget == squeeze_id_lo {
            let new_left = (old_left + ratio_delta).clamp(0.0, old_right - 0.05);
            new_thumb_ratio = (old_right - new_left).clamp(0.05, 1.0);
            if new_thumb_ratio < 1.0 {
                new_scroll = (new_left / (1.0 - new_thumb_ratio)).clamp(0.0, 1.0);
            } else {
                new_scroll = 0.0;
            }
        } else {
            let new_right = (old_right + ratio_delta).clamp(old_left + 0.05, 1.0);
            new_thumb_ratio = (new_right - old_left).clamp(0.05, 1.0);
            if new_thumb_ratio < 1.0 {
                new_scroll = (old_left / (1.0 - new_thumb_ratio)).clamp(0.0, 1.0);
            } else {
                new_scroll = 0.0;
            }
        }
    }

    let new_thumb_ratio_clamped = new_thumb_ratio.clamp(0.05, 1.0);
    let new_thumb_len = ((length as f32 * new_thumb_ratio_clamped) as i32).max(HANDLE_SIZE * 2);
    let new_travel = (length - new_thumb_len).max(1);
    let new_thumb_offset = (new_scroll.clamp(0.0, 1.0) * new_travel as f32) as i32;

    let (lo_draw_rect, hi_draw_rect) = match dir {
        ScrollbarDir::Horizontal => (
            Rect::new(
                x + new_thumb_offset,
                y,
                HANDLE_SIZE as u32,
                thickness as u32,
            ),
            Rect::new(
                x + new_thumb_offset + new_thumb_len - HANDLE_SIZE,
                y,
                HANDLE_SIZE as u32,
                thickness as u32,
            ),
        ),
        ScrollbarDir::Vertical => (
            Rect::new(
                x,
                y + new_thumb_offset,
                thickness as u32,
                HANDLE_SIZE as u32,
            ),
            Rect::new(
                x,
                y + new_thumb_offset + new_thumb_len - HANDLE_SIZE,
                thickness as u32,
                HANDLE_SIZE as u32,
            ),
        ),
    };

    let is_lo_active = input.drag_widget == squeeze_id_lo;
    let is_hi_active = input.drag_widget == squeeze_id_hi;
    let col_lo = if is_lo_active || lo_hover {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.accent_hover)
    };
    let col_hi = if is_hi_active || hi_hover {
        Theme::c(theme.accent)
    } else {
        Theme::c(theme.accent_hover)
    };
    canvas.set_draw_color(col_lo);
    let _ = canvas.fill_rect(lo_draw_rect);
    canvas.set_draw_color(col_hi);
    let _ = canvas.fill_rect(hi_draw_rect);

    // Grip lines
    canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 255, 255, 80));
    match dir {
        ScrollbarDir::Horizontal => {
            let mid_y = y + thickness / 2;
            for dx in [2i32, 4] {
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(lo_draw_rect.x() + dx, mid_y - 2),
                    sdl2::rect::Point::new(lo_draw_rect.x() + dx, mid_y + 2),
                );
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(hi_draw_rect.x() + dx, mid_y - 2),
                    sdl2::rect::Point::new(hi_draw_rect.x() + dx, mid_y + 2),
                );
            }
        }
        ScrollbarDir::Vertical => {
            let mid_x = x + thickness / 2;
            for dy in [2i32, 4] {
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(mid_x - 2, lo_draw_rect.y() + dy),
                    sdl2::rect::Point::new(mid_x + 2, lo_draw_rect.y() + dy),
                );
                let _ = canvas.draw_line(
                    sdl2::rect::Point::new(mid_x - 2, hi_draw_rect.y() + dy),
                    sdl2::rect::Point::new(mid_x + 2, hi_draw_rect.y() + dy),
                );
            }
        }
    }

    (new_scroll, new_thumb_ratio)
}
