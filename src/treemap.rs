//! Squarified treemap layout.

use crate::scan::Tree;
use eframe::egui::{self, pos2, vec2, Rect};

pub struct Cell {
    pub rect: Rect,
    pub node: u32,
    /// Directories with room get a frame and a label strip; everything else is a filled tile.
    pub framed: bool,
}

const HEADER: f32 = 15.0;
const MAX_DEPTH: u32 = 16;

pub fn layout(tree: &Tree, root: u32, area: Rect) -> Vec<Cell> {
    let mut out = Vec::new();
    place(tree, root, area, 0, &mut out);
    out
}

fn place(tree: &Tree, id: u32, rect: Rect, depth: u32, out: &mut Vec<Cell>) {
    let n = &tree.nodes[id as usize];
    let can_split = n.is_dir && !n.children.is_empty() && depth < MAX_DEPTH && rect.width() > 14.0 && rect.height() > 14.0;
    if !can_split {
        out.push(Cell { rect, node: id, framed: false });
        return;
    }
    let with_header = rect.width() > 46.0 && rect.height() > 38.0;
    out.push(Cell { rect, node: id, framed: true });
    let mut inner = rect.shrink(1.5);
    if with_header {
        inner.min.y += HEADER - 1.5;
    }
    if inner.width() < 4.0 || inner.height() < 4.0 {
        return;
    }
    squarify(tree, &n.children, inner, depth + 1, out);
}

fn squarify(tree: &Tree, kids: &[u32], rect: Rect, depth: u32, out: &mut Vec<Cell>) {
    let total: f64 = kids.iter().map(|&k| tree.nodes[k as usize].size as f64).sum();
    if total <= 0.0 {
        return;
    }
    let scale = (rect.width() * rect.height()) as f64 / total;
    let mut rect = rect;
    let mut i = 0;
    while i < kids.len() {
        let side = rect.width().min(rect.height()) as f64;
        if side < 1.0 {
            break;
        }
        // grow the current row while the worst aspect ratio keeps improving
        let (mut sum, mut j, mut worst) = (0.0f64, i, f64::MAX);
        let (mut rmin, mut rmax) = (f64::MAX, 0.0f64);
        while j < kids.len() {
            let a = tree.nodes[kids[j] as usize].size as f64 * scale;
            if a <= 0.0 {
                break;
            }
            let (nsum, nmin, nmax) = (sum + a, rmin.min(a), rmax.max(a));
            let w = (side * side * nmax / (nsum * nsum)).max(nsum * nsum / (side * side * nmin));
            if j > i && w > worst {
                break;
            }
            (sum, rmin, rmax, worst) = (nsum, nmin, nmax, w);
            j += 1;
        }
        if j == i || sum < 0.5 {
            break;
        }
        let horizontal = rect.width() >= rect.height();
        let thick = (sum / side) as f32;
        let mut off = 0.0f32;
        for &k in &kids[i..j] {
            let a = tree.nodes[k as usize].size as f64 * scale;
            let len = (a / sum * side) as f32;
            let r = if horizontal {
                Rect::from_min_size(pos2(rect.min.x, rect.min.y + off), vec2(thick, len))
            } else {
                Rect::from_min_size(pos2(rect.min.x + off, rect.min.y), vec2(len, thick))
            };
            off += len;
            if r.width() >= 1.0 && r.height() >= 1.0 {
                place(tree, k, r, depth, out);
            }
        }
        if horizontal {
            rect.min.x += thick;
        } else {
            rect.min.y += thick;
        }
        i = j;
    }
}

/// Topmost cell under `p` (cells are stored parents-first, so the last hit is the deepest).
pub fn hit(cells: &[Cell], p: egui::Pos2) -> Option<usize> {
    cells.iter().rposition(|c| c.rect.contains(p))
}
