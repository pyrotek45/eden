sed -i -e '/\/\/ Overlap warning — red stripe overlay/,/}/c\
            \/\/ Subtle Overlap warning \
            if overlaps_another {\
                canvas.set_draw_color(sdl2::pixels::Color::RGBA(255, 60, 60, 100));\
                let _ = canvas.draw_rect(Rect::new(cx, clip_y, cw.max(4) as u32, clip_h as u32));\
                let _ = canvas.draw_rect(Rect::new(cx+1, clip_y+1, (cw.max(4).saturating_sub(2)) as u32, (clip_h.saturating_sub(2)) as u32));\
            }' src/views.rs
