//! NoteCraft's own Table tab icons (select cells, rows, columns or the table; delete rows or
//! columns; sort; convert to text), drawn in DeckCraft's style with its helpers on the 24×24
//! grid. Original NoteCraft artwork, `MIT OR Apache-2.0`.

use super::extra::Extra;
use super::geometry::{B, Glyph, K, N, Pen, R, t};

/// The grid every drawing here starts from (left, top, right, bottom), 3 × 3 cells.
const GRID: (f32, f32, f32, f32) = (2.5, 4.0, 21.5, 20.0);

/// One cell of [`GRID`]-like box `g` (`rows` × `cols`).
fn cell(g: (f32, f32, f32, f32), rows: usize, cols: usize, r: usize, c: usize) -> (f32, f32, f32, f32) {
    let (w, h) = ((g.2 - g.0) / cols as f32, (g.3 - g.1) / rows as f32);
    (g.0 + w * c as f32, g.1 + h * r as f32, g.0 + w * (c as f32 + 1.0), g.1 + h * (r as f32 + 1.0))
}

/// Draws the Table tab icons; `false` for the others.
pub(super) fn draw(p: &Pen<'_>, icon: Extra) -> bool {
    match icon {
        Extra::SelectCell | Extra::SelectRow | Extra::SelectColumn | Extra::SelectTable => {
            let fill = t(B);
            let cells: &[(usize, usize)] = match icon {
                Extra::SelectCell => &[(1, 1)],
                Extra::SelectRow => &[(1, 0), (1, 1), (1, 2)],
                Extra::SelectColumn => &[(0, 1), (1, 1), (2, 1)],
                _ => &[(0, 0), (0, 1), (0, 2), (1, 0), (1, 1), (1, 2), (2, 0), (2, 1), (2, 2)],
            };
            for &(r, c) in cells {
                p.rect(cell(GRID, 3, 3, r, c), 0.0, fill, N);
            }
            p.table(GRID, 3, 3, N);
        }
        Extra::DeleteRows | Extra::DeleteColumns => {
            let g = (2.5, 3.0, 19.5, 17.0);
            let cells: &[(usize, usize)] =
                if icon == Extra::DeleteRows { &[(1, 0), (1, 1), (1, 2)] } else { &[(0, 1), (1, 1), (2, 1)] };
            for &(r, c) in cells {
                p.rect(cell(g, 3, 3, r, c), 0.0, t(R), N);
            }
            p.table(g, 3, 3, N);
            p.badge((18.0, 18.0), 5.0, R, Glyph::Cross);
        }
        Extra::SortAscending | Extra::SortDescending => {
            // Lines getting longer (ascending) or shorter, and the arrow beside them.
            let lens = if icon == Extra::SortAscending { [4.0, 7.0, 10.0, 13.0] } else { [13.0, 10.0, 7.0, 4.0] };
            for (i, len) in lens.into_iter().enumerate() {
                let y = 5.0 + i as f32 * 4.7;
                p.seg_w((2.5, y), (2.5 + len, y), K, 1.3);
            }
            p.arrow((19.5, 3.0), (19.5, 21.0), B);
        }
        Extra::ConvertToText => {
            p.table((2.5, 2.5, 13.5, 11.0), 2, 2, N);
            p.arrow((16.0, 6.75), (21.5, 6.75), B);
            for (y, x1) in [(15.0, 21.5), (18.5, 21.5), (22.0, 15.0)] {
                p.seg_w((2.5, y), (x1, y), K, 1.3);
            }
        }
        _ => return false,
    }
    true
}
