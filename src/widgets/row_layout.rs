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
