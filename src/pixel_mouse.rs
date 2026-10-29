//! SGR pixel mouse reporting (DEC mode 1016). Terminals that support it report the
//! pointer in pixels, which gives scrollbar drags sub-cell precision; everything else
//! still sees whole cells.
//!
//! Terminals don't confirm the mode, so reports count as pixels once one lands far
//! outside the cell grid — any pixel report past the top-left corner does.

use crossterm::event::MouseEvent;

pub const ENABLE: &str = "\x1b[?1016h";
pub const DISABLE: &str = "\x1b[?1016l";

#[derive(Debug, Default)]
pub struct PixelMouse {
    confirmed: bool,
}

impl PixelMouse {
    /// Convert `m` to cells in place, given the grid size `(cols, rows)` and window size
    /// `(width, height)` in pixels. Returns the fractional row once pixel reports are
    /// confirmed.
    pub fn convert(
        &mut self,
        m: &mut MouseEvent,
        (cols, rows): (u16, u16),
        (width, height): (u16, u16),
    ) -> Option<f32> {
        if cols == 0 || rows == 0 || width == 0 || height == 0 {
            return None;
        }
        // shortcut: before the first report outside twice the grid, pixel reports from the
        // top-left corner read as cells; any pointer motion elsewhere confirms the mode.
        self.confirmed |= m.column >= cols.saturating_mul(2) || m.row >= rows.saturating_mul(2);
        if !self.confirmed {
            return None;
        }
        // Cells are whole pixels; the window size may include the terminal's padding
        // (e.g. kitty's `window_padding_width`), which flooring drops.
        // shortcut: padding wider than one pixel per column/row still skews clicks.
        let cell_w = f32::from((width / cols).max(1));
        let cell_h = f32::from((height / rows).max(1));
        let fine_row = f32::from(m.row) / cell_h;
        m.column = ((f32::from(m.column) / cell_w) as u16).min(cols - 1);
        m.row = (fine_row as u16).min(rows - 1);
        Some(fine_row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyModifiers, MouseEventKind};

    fn at(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Moved,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn cells_pass_through_until_a_pixel_report_confirms() {
        // 100×40 grid of 10×20 px cells.
        let (grid, px) = ((100, 40), (1000, 800));
        let mut pm = PixelMouse::default();
        let mut m = at(50, 30);
        assert_eq!(pm.convert(&mut m, grid, px), None);
        assert_eq!((m.column, m.row), (50, 30));

        let mut m = at(505, 610);
        assert_eq!(pm.convert(&mut m, grid, px), Some(30.5));
        assert_eq!((m.column, m.row), (50, 30));
        // Confirmed: small coordinates are pixels too now.
        let mut m = at(15, 10);
        assert_eq!(pm.convert(&mut m, grid, px), Some(0.5));
        assert_eq!((m.column, m.row), (1, 0));
    }

    #[test]
    fn window_padding_does_not_shift_cells() {
        // 140×40 grid of 10×20 px cells inside 10 px of padding on every side.
        let (grid, px) = ((140, 40), (1420, 820));
        let mut pm = PixelMouse::default();
        let mut m = at(705, 410);
        assert_eq!(pm.convert(&mut m, grid, px), Some(20.5));
        assert_eq!((m.column, m.row), (70, 20));
        let mut m = at(1395, 0);
        pm.convert(&mut m, grid, px);
        assert_eq!(m.column, 139);
    }
}
