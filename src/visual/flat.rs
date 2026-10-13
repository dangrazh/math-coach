//! 2D visualizations: pie chart and a right isosceles triangle tiled by smaller ones.

use std::collections::HashMap;
use std::f64::consts::TAU;

use super::grouping::Partition;
use super::{outline, polygon, segments, shape, Point, Rgb, Style};
use crate::Shape;

const PIE_RADIUS: f64 = 100.0;
/// Hypotenuse (base) of the large triangle; its height is half of that.
const TRIANGLE_BASE: f64 = 224.0;
const PIECE_STROKE: Rgb = Rgb(255, 255, 255);
const OUTLINE_WIDTH: f32 = 3.0;

fn flat_piece(commands: String, style: Style, pieces: usize) -> Shape {
    let width = if pieces > 60 { 1.0 } else { 2.0 };
    shape(commands, style.color().opaque(), PIECE_STROKE.opaque(), width)
}

/// Point on the pie rim, `angle` measured clockwise from 12 o'clock.
fn on_rim(center: Point, angle: f64) -> Point {
    Point { x: center.x + PIE_RADIUS * angle.sin(), y: center.y - PIE_RADIUS * angle.cos() }
}

/// A pie cut into `sectors` equal sectors, clockwise from 12 o'clock.
pub fn pie(sectors: u32, styles: &[Style], center: Point) -> Vec<Shape> {
    let r = PIE_RADIUS;
    let Point { x: cx, y: cy } = center;

    if sectors == 1 {
        let (top, bottom) = (cy - r, cy + r);
        let circle = format!("M {cx} {top} A {r} {r} 0 1 1 {cx} {bottom} A {r} {r} 0 1 1 {cx} {top} Z");
        return vec![flat_piece(circle, styles[0], 1)];
    }

    // With at least two sectors each one spans at most 180°, so the small-arc flag is always 0.
    let step = TAU / f64::from(sectors);
    let mut shapes: Vec<Shape> = styles
        .iter()
        .enumerate()
        .map(|(i, &style)| {
            let start = on_rim(center, i as f64 * step);
            let end = on_rim(center, (i + 1) as f64 * step);
            let sector = format!(
                "M {cx} {cy} L {:.2} {:.2} A {r} {r} 0 0 1 {:.2} {:.2} Z",
                start.x, start.y, end.x, end.y
            );
            flat_piece(sector, style, styles.len())
        })
        .collect();

    // Group outline: the rim plus a radius wherever a new group starts.
    if styles.iter().all(|s| s.group.is_some()) {
        let n = styles.len();
        let radii = (0..n)
            .filter(|&i| styles[i].group != styles[(i + n - 1) % n].group)
            .map(|i| [center, on_rim(center, i as f64 * step)]);
        let (top, bottom) = (cy - r, cy + r);
        let rim = format!("M {cx} {top} A {r} {r} 0 1 1 {cx} {bottom} A {r} {r} 0 1 1 {cx} {top} ");
        shapes.push(outline(rim + &segments(radii), OUTLINE_WIDTH));
    }
    shapes
}

/// A right isosceles triangle (right angle at the top, hypotenuse at the bottom) tiled by
/// `rows²` congruent right isosceles triangles, each optionally halved along its
/// altitude into two smaller right isosceles triangles (`2 · rows²` pieces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriangleGrid {
    rows: u32,
    halved: bool,
}

struct Piece {
    corners: [Point; 3],
    row: u32,
}

impl Piece {
    fn centroid(&self) -> Point {
        let [a, b, c] = self.corners;
        Point { x: (a.x + b.x + c.x) / 3.0, y: (a.y + b.y + c.y) / 3.0 }
    }

    fn contains(&self, p: Point) -> bool {
        let [a, b, c] = self.corners;
        let side = |u: Point, v: Point| (v.x - u.x) * (p.y - u.y) - (v.y - u.y) * (p.x - u.x);
        let (d1, d2, d3) = (side(a, b), side(b, c), side(c, a));
        !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
    }
}

impl TriangleGrid {
    /// Only `k²` and `2·k²` pieces can tile the triangle with congruent right isosceles triangles.
    pub fn for_count(count: u32) -> Option<Self> {
        let exact_sqrt = |n: u32| {
            let root = f64::from(n).sqrt().round() as u32;
            (root * root == n).then_some(root)
        };
        match exact_sqrt(count) {
            Some(rows) if rows > 0 => Some(Self { rows, halved: false }),
            _ if count.is_multiple_of(2) => exact_sqrt(count / 2).filter(|&r| r > 0).map(|rows| Self { rows, halved: true }),
            _ => None,
        }
    }

    pub fn count(self) -> u32 {
        self.rows * self.rows * if self.halved { 2 } else { 1 }
    }

    /// The pieces row by row from the top, left to right within a row.
    fn pieces(self, center: Point) -> Vec<Piece> {
        let height = TRIANGLE_BASE / 2.0;
        let apex = Point { x: center.x, y: center.y - height / 2.0 };
        let side = TRIANGLE_BASE / f64::from(self.rows);
        let row_height = height / f64::from(self.rows);
        // j-th lattice point on the horizontal line `row` (which holds `row + 1` points).
        let lattice = |row: u32, j: f64| Point {
            x: apex.x + (j - f64::from(row) / 2.0) * side,
            y: apex.y + f64::from(row) * row_height,
        };

        let mut pieces = Vec::with_capacity(self.count() as usize);
        for row in 0..self.rows {
            for k in 0..2 * row + 1 {
                let j = f64::from(k / 2);
                // Pointing up: right angle at the top. Pointing down: right angle at the bottom.
                let (corners, halves) = if k % 2 == 0 {
                    let (top, left, right) = (lattice(row, j), lattice(row + 1, j), lattice(row + 1, j + 1.0));
                    let mid = lattice(row + 1, j + 0.5);
                    ([top, left, right], [[top, left, mid], [top, mid, right]])
                } else {
                    let (left, right, bottom) = (lattice(row, j), lattice(row, j + 1.0), lattice(row + 1, j + 1.0));
                    let mid = lattice(row, j + 0.5);
                    ([left, right, bottom], [[left, mid, bottom], [mid, right, bottom]])
                };
                if self.halved {
                    pieces.extend(halves.map(|corners| Piece { corners, row }));
                } else {
                    pieces.push(Piece { corners, row });
                }
            }
        }
        pieces
    }

    /// Pairs of pieces sharing an edge (i.e. two corners).
    pub fn adjacency(self) -> Vec<(usize, usize)> {
        edges(&self.pieces(Point { x: 0.0, y: 0.0 }))
            .into_iter()
            .filter_map(|edge| match edge.pieces[..] {
                [a, b] => Some((a, b)),
                _ => None,
            })
            .collect()
    }

    pub fn partitions(self, group_size: u32) -> Vec<Partition> {
        let origin = Point { x: 0.0, y: 0.0 };
        let pieces = self.pieces(origin);
        let size = group_size as usize;
        let mut partitions = Vec::new();

        // Larger triangles of the same kind, each made of `group_size` small ones; only
        // possible when the coarse grid's lines are also lines of the fine grid.
        if let Some(coarse) = TriangleGrid::for_count(self.count() / group_size) {
            let coarse_pieces = coarse.pieces(origin);
            let group_of: Option<Vec<u32>> = pieces
                .iter()
                .map(|p| coarse_pieces.iter().position(|c| c.contains(p.centroid())).map(|g| g as u32))
                .collect();
            if let Some(group_of) = group_of {
                let partition = Partition { label: "Grössere Dreiecke".into(), group_of };
                if partition.is_balanced(size) {
                    partitions.push(partition);
                }
            }
        }

        // Row by row, alternating direction so every group stays in one piece.
        let mut order: Vec<usize> = Vec::with_capacity(pieces.len());
        for row in 0..self.rows {
            let mut in_row: Vec<usize> = (0..pieces.len()).filter(|&i| pieces[i].row == row).collect();
            if row % 2 == 1 {
                in_row.reverse();
            }
            order.extend(in_row);
        }
        partitions.push(Partition::from_order("Zeilenweise (Schlange)", &order, size));
        partitions
    }
}

/// An edge of the triangle grid and the one or two pieces it belongs to.
struct Edge {
    ends: [Point; 2],
    pieces: Vec<usize>,
}

fn edges(pieces: &[Piece]) -> Vec<Edge> {
    // Corners rounded to 1/100 unit, so shared corners compare equal.
    type Key = (i64, i64);
    let key = |p: Point| -> Key { ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64) };
    let mut by_ends: HashMap<(Key, Key), Edge> = HashMap::new();
    for (i, piece) in pieces.iter().enumerate() {
        let [a, b, c] = piece.corners;
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let (ku, kv) = (key(u), key(v));
            by_ends
                .entry((ku.min(kv), ku.max(kv)))
                .or_insert_with(|| Edge { ends: [u, v], pieces: Vec::new() })
                .pieces
                .push(i);
        }
    }
    by_ends.into_values().collect()
}

pub fn triangle(grid: TriangleGrid, styles: &[Style], center: Point) -> Vec<Shape> {
    let pieces = grid.pieces(center);
    let mut shapes: Vec<Shape> = pieces
        .iter()
        .zip(styles)
        .map(|(piece, &style)| flat_piece(polygon(&piece.corners), style, styles.len()))
        .collect();

    // Group outline: the outer border plus every edge between two different groups.
    if styles.iter().all(|s| s.group.is_some()) {
        let borders = edges(&pieces).into_iter().filter_map(|edge| match edge.pieces[..] {
            [a, b] if styles[a].group == styles[b].group => None,
            _ => Some(edge.ends),
        });
        shapes.push(outline(segments(borders), OUTLINE_WIDTH));
    }
    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area([a, b, c]: [Point; 3]) -> f64 {
        ((b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)).abs() / 2.0
    }

    fn sq_dist(a: Point, b: Point) -> f64 {
        (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
    }

    #[test]
    fn valid_triangle_counts() {
        let valid: Vec<u32> = (1..=50).filter(|&n| TriangleGrid::for_count(n).is_some()).collect();
        assert_eq!(valid, [1, 2, 4, 8, 9, 16, 18, 25, 32, 36, 49, 50]);
    }

    #[test]
    fn pieces_are_congruent_right_isosceles_triangles() {
        for count in [1, 2, 9, 18, 32] {
            let grid = TriangleGrid::for_count(count).unwrap();
            let pieces = grid.pieces(Point { x: 0.0, y: 0.0 });
            assert_eq!(pieces.len(), count as usize);

            let expected_area = TRIANGLE_BASE * TRIANGLE_BASE / 4.0 / f64::from(count);
            for piece in &pieces {
                assert!((area(piece.corners) - expected_area).abs() < 1e-6);
                let [a, b, c] = piece.corners;
                let mut sides = [sq_dist(a, b), sq_dist(b, c), sq_dist(c, a)];
                sides.sort_by(f64::total_cmp);
                // Two equal legs, and Pythagoras for the right angle.
                assert!((sides[0] - sides[1]).abs() < 1e-6);
                assert!((sides[0] + sides[1] - sides[2]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn coarse_triangles_are_offered_when_grids_line_up() {
        let labels = |count, groups| -> Vec<String> {
            let grid = TriangleGrid::for_count(count).unwrap();
            grid.partitions(count / groups).into_iter().map(|p| p.label).collect()
        };
        assert_eq!(labels(64, 4), ["Grössere Dreiecke", "Zeilenweise (Schlange)"]);
        assert_eq!(labels(32, 8), ["Grössere Dreiecke", "Zeilenweise (Schlange)"]);
        // 8 coarse halves cannot be built from an unhalved 8×8 grid.
        assert_eq!(labels(64, 8), ["Zeilenweise (Schlange)"]);
    }

    #[test]
    fn adjacency_matches_grid() {
        // 4 triangles: the middle (down) one touches the other three.
        let mut pairs = TriangleGrid::for_count(4).unwrap().adjacency();
        pairs.iter_mut().for_each(|p| *p = (p.0.min(p.1), p.0.max(p.1)));
        pairs.sort_unstable();
        assert_eq!(pairs, [(0, 2), (1, 2), (2, 3)]);
    }
}
