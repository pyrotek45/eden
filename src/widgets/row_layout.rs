// Eden DAW — Row Layout container

/// Positions a list of buttons (or other fixed-width items) in a horizontal row.
pub struct RowLayout {
    pub x: i32,
    pub y: i32,
    pub total_width: i32,
    pub height: i32,
    pub gap: i32,
}

/// Describes one slot in the row.
pub struct RowItem {
    pub width: i32,
    pub can_resize: bool,
    pub min_width: i32,
}

impl RowLayout {
    /// Compute `(x_pos, actual_width)` for every item.
    pub fn layout(&self, items: &[RowItem]) -> Vec<(i32, i32)> {
        let n = items.len();
        if n == 0 {
            return Vec::new();
        }
        let total_gap = self.gap * (n as i32 - 1);
        let fixed_total: i32 = items
            .iter()
            .filter(|it| !it.can_resize)
            .map(|it| it.width)
            .sum();
        let resize_count = items.iter().filter(|it| it.can_resize).count() as i32;
        let leftover = (self.total_width - fixed_total - total_gap).max(0);
        let resize_each = if resize_count > 0 {
            (leftover / resize_count).max(0)
        } else {
            0
        };

        let mut out = Vec::with_capacity(n);
        let mut cursor = self.x;
        for item in items {
            let w = if item.can_resize {
                resize_each.max(item.min_width)
            } else {
                item.width
            };
            out.push((cursor, w));
            cursor += w + self.gap;
        }
        out
    }

    /// Like `layout` but fills from the RIGHT edge.
    pub fn layout_right(&self, items: &[RowItem]) -> Vec<(i32, i32)> {
        let n = items.len();
        if n == 0 {
            return Vec::new();
        }
        let total_gap = self.gap * (n as i32 - 1);
        let fixed_total: i32 = items
            .iter()
            .filter(|it| !it.can_resize)
            .map(|it| it.width)
            .sum();
        let resize_count = items.iter().filter(|it| it.can_resize).count() as i32;
        let leftover = (self.total_width - fixed_total - total_gap).max(0);
        let resize_each = if resize_count > 0 {
            (leftover / resize_count).max(0)
        } else {
            0
        };

        let mut out = vec![(0i32, 0i32); n];
        let mut cursor = self.x + self.total_width;
        for (i, item) in items.iter().enumerate() {
            let w = if item.can_resize {
                resize_each.max(item.min_width)
            } else {
                item.width
            };
            cursor -= w;
            out[i] = (cursor, w);
            cursor -= self.gap;
        }
        out
    }
}

/// Top actions wrap below the transport controls instead of overlapping them.
/// Entries are (x, width, y), in rightmost-first order.
pub fn transport_action_layout(width: i32, font_scale: i32, keyboard_mode: bool) -> (Vec<(i32, i32, i32)>, i32) {
    let mut labels = vec!["Home", "Options", "Save", "Export", "Project", "Redo", "Undo", "KBD", "STOP", "?"];
    if keyboard_mode { labels.push("< C0 >"); }
    let mut x = width - 10;
    let mut y = 9;
    let mut slots = Vec::with_capacity(labels.len());
    for label in labels {
        let item_w = (label.chars().count() as i32 * (4 * font_scale + 1) + 16).max(30);
        let left = if y == 9 { 506 } else { 10 };
        if x - item_w < left {
            y += 40;
            x = width - 10;
        }
        x -= item_w;
        slots.push((x, item_w, y));
        x -= 4;
    }
    (slots, y + 39)
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    #[test]
    fn toolbar_reflows_without_covering_transport_or_other_actions() {
        for width in [520, 640, 800, 1024, 1280, 1920] {
            for font in 1..=4 {
                for keyboard in [false, true] {
                    let (slots, height) = transport_action_layout(width, font, keyboard);
                    for (index, &(x, w, y)) in slots.iter().enumerate() {
                        assert!(x >= 10 && x + w <= width - 10);
                        assert!(y != 9 || x >= 506);
                        assert!(y + 30 < height);
                        for &(other_x, other_w, other_y) in &slots[..index] {
                            assert!(y != other_y || x + w <= other_x || other_x + other_w <= x);
                        }
                    }
                }
            }
        }
    }
}
