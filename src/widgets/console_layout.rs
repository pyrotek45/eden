//! Console-inspired grouping of the existing CStrip2 controls; no DSP remapping.
#[derive(Clone, Copy)]
pub struct ChannelCell {
    pub id: &'static str,
    pub caption: &'static str,
    pub x: i32,
    pub y: i32,
}
pub struct ChannelSection {
    pub label: &'static str,
    pub y: i32,
    pub height: i32,
}
pub struct ChannelLayout {
    pub sections: Vec<ChannelSection>,
    pub cells: Vec<ChannelCell>,
    pub height: i32,
    pub row_height: i32,
}

pub fn channel_layout(width: i32, font: i32) -> ChannelLayout {
    let title_height = 5 * font + 4;
    let row_height = (5 * font + 28).max(40);
    let groups: [(&str, &[(&str, &str, usize, usize)]); 3] = [
        (
            "FILTER",
            &[("lo_cap", "LO CUT", 0, 0), ("hi_cap", "HI CUT", 1, 0)],
        ),
        (
            "EQ",
            &[
                ("treble", "HF", 0, 0),
                ("treb_frq", "HF FREQ", 1, 0),
                ("mid", "MID", 0, 1),
                ("bass", "LF", 0, 2),
                ("bass_frq", "LF FREQ", 1, 2),
            ],
        ),
        (
            "DYNAMICS / TRIM",
            &[
                ("compress", "COMP", 0, 0),
                ("comp_spd", "SPEED", 1, 0),
                ("output", "TRIM", 2, 0),
            ],
        ),
    ];
    let mut sections = Vec::new();
    let mut cells = Vec::new();
    let mut y = 0;
    for (label, params) in groups {
        let columns = if label == "DYNAMICS / TRIM" { 3 } else { 2 };
        let rows = params.iter().map(|(_, _, _, r)| *r).max().unwrap_or(0) + 1;
        let height = title_height + rows as i32 * row_height + 4;
        sections.push(ChannelSection { label, y, height });
        for &(id, caption, col, row) in params {
            let cell_w = width / columns;
            cells.push(ChannelCell {
                id,
                caption,
                x: cell_w * col as i32 + cell_w / 2,
                y: y + title_height + row as i32 * row_height,
            });
        }
        y += height + 6;
    }
    ChannelLayout {
        sections,
        cells,
        height: y - 6,
        row_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn console_contains_every_real_channel_parameter_once() {
        let layout = channel_layout(184, 2);
        let expected = crate::modules::get_param_descs("CStrip2");
        assert_eq!(layout.cells.len(), expected.len());
        for parameter in expected {
            assert_eq!(
                layout.cells.iter().filter(|c| c.id == parameter.id).count(),
                1
            );
        }
        for cell in &layout.cells {
            assert!(cell.x >= 12 && cell.x <= 172);
            assert!(cell.y + layout.row_height <= layout.height);
        }
        assert_eq!(
            layout.sections.iter().map(|s| s.label).collect::<Vec<_>>(),
            vec!["FILTER", "EQ", "DYNAMICS / TRIM"]
        );
    }
}
