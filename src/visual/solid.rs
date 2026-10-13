//! 3D visualizations (board of tiles, cube of cubes) rendered with a tiny
//! orthographic projector and the painter's algorithm.
//!
//! World space: x to the right, y away from the viewer, z up.

use super::{outline, polygon, segments, shape, Point, Rgb, Style, CELL};
use crate::Shape;

type V3 = [f64; 3];

/// Rotation of the 3D object in degrees, both normalized to `0..360`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub rot_x: f64,
    pub rot_z: f64,
}

impl View {
    pub fn new(rot_x: f64, rot_z: f64) -> Self {
        Self { rot_x: rot_x.rem_euclid(360.0), rot_z: rot_z.rem_euclid(360.0) }
    }
}

const PLATE: Rgb = Rgb(148, 163, 184);
const GLASS_EDGE: Rgb = Rgb(100, 116, 139);
const OUTLINE_WIDTH: f32 = 2.5;

/// Direction towards the light in view space (from the upper left, in front).
const LIGHT: V3 = [-0.4385, -0.7675, 0.4385];

fn dot(a: V3, b: V3) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn length(v: V3) -> f64 {
    dot(v, v).sqrt()
}

/// `Rx(rot_x) * Rz(rot_z)`: spin around the vertical axis first, then tilt.
struct Rotation([V3; 3]);

impl Rotation {
    fn new(view: View) -> Self {
        let (sx, cx) = view.rot_x.to_radians().sin_cos();
        let (sz, cz) = view.rot_z.to_radians().sin_cos();
        Self([[cz, -sz, 0.0], [cx * sz, cx * cz, -sx], [sx * sz, sx * cz, cx]])
    }

    fn apply(&self, v: V3) -> V3 {
        self.0.map(|row| dot(row, v))
    }
}

/// Maps world coordinates of one object onto its cell in the canvas.
struct Projector {
    rotation: Rotation,
    origin: V3,
    scale: f64,
    screen: Point,
}

impl Projector {
    /// Centers an object of the given `extent` (whose minimum corner is `min`) in its cell
    /// and scales it so it stays inside the cell at every rotation.
    fn new(view: View, min: V3, extent: V3, screen: Point) -> Self {
        let origin = [0, 1, 2].map(|i| min[i] + extent[i] / 2.0);
        let radius = length(extent) / 2.0;
        Self { rotation: Rotation::new(view), origin, scale: CELL * 0.45 / radius, screen }
    }

    fn to_view(&self, p: V3) -> V3 {
        self.rotation.apply([0, 1, 2].map(|i| p[i] - self.origin[i]))
    }

    fn project(&self, p: V3) -> Point {
        let [x, _, z] = self.to_view(p);
        Point { x: self.screen.x + self.scale * x, y: self.screen.y - self.scale * z }
    }

    fn depth(&self, p: V3) -> f64 {
        self.to_view(p)[1]
    }

    /// Whether a surface with the given world normal points towards the viewer.
    fn faces_viewer(&self, normal: V3) -> bool {
        self.rotation.apply(normal)[1] < 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Material {
    /// Opaque, back faces culled.
    Solid(Rgb),
    /// Translucent, back faces drawn too so the inside stays visible.
    Glass(Rgb),
}

/// The six faces of an axis-aligned box as `(axis, positive side)`.
const FACES: [(usize, bool); 6] = [(0, false), (0, true), (1, false), (1, true), (2, false), (2, true)];

fn face_normal((axis, positive): (usize, bool)) -> V3 {
    let mut n = [0.0; 3];
    n[axis] = if positive { 1.0 } else { -1.0 };
    n
}

#[derive(Debug, Clone, Copy)]
struct Block {
    min: V3,
    max: V3,
    material: Material,
    /// Which entries of [`FACES`] to draw.
    visible_faces: [bool; 6],
    /// Per face, which of its four edges (bit `k` = edge from corner `k` to `k + 1`
    /// of [`Block::face_corners`]) carry a group outline.
    outlined_edges: [u8; 6],
}

impl Block {
    fn new(min: V3, size: V3, material: Material) -> Self {
        let max = [0, 1, 2].map(|i| min[i] + size[i]);
        Self { min, max, material, visible_faces: [true; 6], outlined_edges: [0; 6] }
    }

    fn center(&self) -> V3 {
        [0, 1, 2].map(|i| (self.min[i] + self.max[i]) / 2.0)
    }

    fn face_corners(&self, (axis, positive): (usize, bool)) -> [V3; 4] {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        let fixed = if positive { self.max[axis] } else { self.min[axis] };
        [(false, false), (true, false), (true, true), (false, true)].map(|(at_max_u, at_max_v)| {
            let mut p = [0.0; 3];
            p[axis] = fixed;
            p[u] = if at_max_u { self.max[u] } else { self.min[u] };
            p[v] = if at_max_v { self.max[v] } else { self.min[v] };
            p
        })
    }
}

fn face_shape(material: Material, corners: [Point; 4], light: f64, front: bool) -> Shape {
    let commands = polygon(&corners);
    match material {
        Material::Solid(color) => {
            let brightness = 0.55 + 0.45 * light;
            shape(commands, color.shaded(brightness, 255), color.shaded(brightness * 0.65, 255), 1.0)
        }
        Material::Glass(color) => {
            let edge_alpha = if front { 170 } else { 70 };
            shape(commands, color.shaded(1.0, 60), GLASS_EDGE.shaded(1.0, edge_alpha), 1.0)
        }
    }
}

/// Draws blocks back to front. Sorting whole blocks by center depth is exact for the
/// non-overlapping, equally sized grid cells used here.
fn render_blocks(blocks: &[Block], projector: &Projector) -> Vec<Shape> {
    let mut ordered: Vec<(f64, &Block)> = blocks.iter().map(|b| (projector.depth(b.center()), b)).collect();
    ordered.sort_by(|(a, _), (b, _)| b.total_cmp(a));

    let mut shapes = Vec::new();
    for (_, block) in ordered {
        let mut faces: Vec<(bool, Shape)> = FACES
            .iter()
            .zip(block.visible_faces)
            .filter(|&(_, visible)| visible)
            .filter_map(|(&face, _)| {
                let normal = face_normal(face);
                let front = projector.faces_viewer(normal);
                if !front && matches!(block.material, Material::Solid(_)) {
                    return None;
                }
                let light = dot(projector.rotation.apply(normal), LIGHT).max(0.0);
                let corners = block.face_corners(face).map(|p| projector.project(p));
                Some((front, face_shape(block.material, corners, light, front)))
            })
            .collect();
        // Back faces (only present for glass) go underneath the front faces.
        faces.sort_by_key(|(front, _)| *front);
        shapes.extend(faces.into_iter().map(|(_, shape)| shape));

        // Group outlines on the block's visible front faces, drawn right after the block
        // so nearer blocks still cover them.
        let lines: Vec<[Point; 2]> = FACES
            .iter()
            .zip(block.visible_faces.iter().zip(block.outlined_edges))
            .filter(|&(&face, (&visible, mask))| visible && mask != 0 && projector.faces_viewer(face_normal(face)))
            .flat_map(|(&face, (_, mask))| {
                let corners = block.face_corners(face).map(|p| projector.project(p));
                (0..4).filter(move |k| mask & (1 << k) != 0).map(move |k| [corners[k], corners[(k + 1) % 4]])
            })
            .collect();
        if !lines.is_empty() {
            shapes.push(outline(segments(lines), OUTLINE_WIDTH));
        }
    }
    shapes
}

/// In-plane direction pointing out of edge `k` of a face, as `(axis, positive)`.
/// Matches the corner order of [`Block::face_corners`].
fn edge_direction((axis, _): (usize, bool), k: usize) -> (usize, bool) {
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    [(v, false), (u, true), (v, true), (u, false)][k]
}

/// The most square `rows × cols` grid holding exactly `n` tiles (`rows <= cols`).
pub fn board_dims(n: u32) -> (u32, u32) {
    let rows = (1..=n).take_while(|r| r * r <= n).filter(|r| n.is_multiple_of(*r)).last().unwrap_or(1);
    (rows, n / rows)
}

/// The most cube-like `[x, y, z]` box holding exactly `n` cubes (`x >= y >= z`).
pub fn cuboid_dims(n: u32) -> [u32; 3] {
    let mut best = [n, 1, 1];
    for a in (1..=n).take_while(|a| a * a * a <= n).filter(|a| n.is_multiple_of(*a)) {
        let rest = n / a;
        for b in (a..=rest).take_while(|b| b * b <= rest).filter(|b| rest.is_multiple_of(*b)) {
            let c = rest / b;
            if c - a < best[0] - best[2] {
                best = [c, b, a];
            }
        }
    }
    best
}

/// A base plate carrying `rows × cols` tiles, indexed row by row from the back
/// (reading order when looking at the board from the front).
pub fn board(rows: u32, cols: u32, styles: &[Style], view: View, center: Point) -> Vec<Shape> {
    const GAP: f64 = 0.12;
    const TILE_SIDE: f64 = 1.0 - GAP;
    const TILE_HEIGHT: f64 = TILE_SIDE / 3.0;
    const PLATE_HEIGHT: f64 = 0.15;
    const MARGIN: f64 = 0.15;

    let (rows_f, cols_f) = (f64::from(rows), f64::from(cols));

    let plate_min = [-MARGIN, -MARGIN, -PLATE_HEIGHT];
    let extent = [cols_f + 2.0 * MARGIN, rows_f + 2.0 * MARGIN, PLATE_HEIGHT + TILE_HEIGHT];
    let projector = Projector::new(view, plate_min, extent, center);

    let plate = Block::new(plate_min, [extent[0], extent[1], PLATE_HEIGHT], Material::Solid(PLATE));
    let tiles: Vec<Block> = styles
        .iter()
        .enumerate()
        .map(|(i, style)| {
            let (row, col) = (i as u32 / cols, i as u32 % cols);
            let min = [f64::from(col) + GAP / 2.0, f64::from(rows - 1 - row) + GAP / 2.0, 0.0];
            Block::new(min, [TILE_SIDE, TILE_SIDE, TILE_HEIGHT], Material::Solid(style.color()))
        })
        .collect();

    // The plate and the tiles are separated by the plane z = 0, so whichever side
    // the viewer is on decides which is drawn first.
    let plate_shapes = render_blocks(&[plate], &projector);
    let tile_shapes = render_blocks(&tiles, &projector);
    let from_above = projector.faces_viewer([0.0, 0.0, 1.0]);
    let mut shapes = if from_above {
        [plate_shapes, tile_shapes].concat()
    } else {
        [tile_shapes, plate_shapes].concat()
    };

    // Group outlines run through the gaps at tile-top height (or under the plate when seen
    // from below); nothing rises above that plane, so they can be drawn last.
    if styles.iter().all(|s| s.group.is_some()) {
        let z = if from_above { TILE_HEIGHT } else { -PLATE_HEIGHT };
        let (rows_i, cols_i) = (i64::from(rows), i64::from(cols));
        let group_at = |row: i64, col: i64| {
            ((0..rows_i).contains(&row) && (0..cols_i).contains(&col)).then(|| styles[(row * cols_i + col) as usize].group)
        };
        let mut lines = Vec::new();
        for (i, style) in styles.iter().enumerate() {
            let (row, col) = (i as i64 / cols_i, i as i64 % cols_i);
            let (x, y) = (col as f64, (rows_i - 1 - row) as f64);
            // (neighbor, shared side in world x/y); row 0 is at the back (largest y).
            let sides = [
                ((row - 1, col), [[x, y + 1.0], [x + 1.0, y + 1.0]]),
                ((row + 1, col), [[x, y], [x + 1.0, y]]),
                ((row, col - 1), [[x, y], [x, y + 1.0]]),
                ((row, col + 1), [[x + 1.0, y], [x + 1.0, y + 1.0]]),
            ];
            for (neighbor, ends) in sides {
                let draw = match group_at(neighbor.0, neighbor.1) {
                    None => true,
                    // Inner sides are drawn once, from the cell with the smaller index.
                    Some(other) => other != style.group && neighbor > (row, col),
                };
                if draw {
                    lines.push(ends.map(|[px, py]| projector.project([px, py, z])));
                }
            }
        }
        shapes.push(outline(segments(lines), OUTLINE_WIDTH));
    }
    shapes
}

/// A box of `x × y × z` small cubes, indexed with x fastest and z (bottom to top) slowest.
/// Filled cubes are solid, empty ones are glass so the filled ones stay visible.
pub fn cube(dims: [u32; 3], styles: &[Style], view: View, center: Point) -> Vec<Shape> {
    let [nx, ny, nz] = dims.map(i64::from);
    let index = |[x, y, z]: [i64; 3]| ((z * ny + y) * nx + x) as usize;
    let inside = |[x, y, z]: [i64; 3]| (0..nx).contains(&x) && (0..ny).contains(&y) && (0..nz).contains(&z);

    let projector = Projector::new(view, [0.0; 3], dims.map(f64::from), center);

    let blocks: Vec<Block> = (0..nz)
        .flat_map(|z| (0..ny).flat_map(move |y| (0..nx).map(move |x| [x, y, z])))
        .map(|cell| {
            let style = styles[index(cell)];
            let material = if style.filled { Material::Solid(style.color()) } else { Material::Glass(style.color()) };
            let mut block = Block::new(cell.map(|c| c as f64), [1.0; 3], material);
            // Only draw a face where it separates this cube from the outside, a filled cube
            // from an empty one, or two empty groups.
            // Faces inside a solid region would be hidden anyway.
            block.visible_faces = FACES.map(|(axis, positive)| {
                let mut neighbor = cell;
                neighbor[axis] += if positive { 1 } else { -1 };
                if !inside(neighbor) {
                    return true;
                }
                let other = styles[index(neighbor)];
                match (style.filled, other.filled) {
                    (true, false) => true,
                    (false, false) => style.group != other.group,
                    _ => false,
                }
            });
            // Outline a face edge where the surface continues into another group or ends.
            if style.group.is_some() {
                block.outlined_edges = FACES.map(|face| {
                    (0..4).fold(0, |mask, k| {
                        let (axis, positive) = edge_direction(face, k);
                        let mut neighbor = cell;
                        neighbor[axis] += if positive { 1 } else { -1 };
                        let boundary = !inside(neighbor) || styles[index(neighbor)].group != style.group;
                        if boundary { mask | (1 << k) } else { mask }
                    })
                });
            }
            block
        })
        .collect();

    render_blocks(&blocks, &projector)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_wraps_angles() {
        assert_eq!(View::new(370.0, -30.0), View { rot_x: 10.0, rot_z: 330.0 });
    }

    #[test]
    fn board_dims_are_most_square() {
        assert_eq!(board_dims(1), (1, 1));
        assert_eq!(board_dims(7), (1, 7));
        assert_eq!(board_dims(12), (3, 4));
        assert_eq!(board_dims(16), (4, 4));
    }

    #[test]
    fn cuboid_dims_are_most_cube_like() {
        assert_eq!(cuboid_dims(1), [1, 1, 1]);
        assert_eq!(cuboid_dims(7), [7, 1, 1]);
        assert_eq!(cuboid_dims(8), [2, 2, 2]);
        assert_eq!(cuboid_dims(12), [3, 2, 2]);
        assert_eq!(cuboid_dims(27), [3, 3, 3]);
        for n in 1..=100 {
            assert_eq!(cuboid_dims(n).iter().product::<u32>(), n);
        }
    }

    fn styles(count: u32, filled: u32) -> Vec<Style> {
        (0..count).map(|i| Style { filled: i < filled, tint: 0, group: None }).collect()
    }

    #[test]
    fn shapes_stay_inside_cell_at_any_rotation() {
        let center = Point { x: CELL / 2.0, y: CELL / 2.0 };
        for (rot_x, rot_z) in [(0.0, 0.0), (45.0, 45.0), (135.0, 300.0), (270.0, 90.0)] {
            let view = View::new(rot_x, rot_z);
            for (rows, cols, filled) in [(1, 7, 3), (3, 4, 5), (8, 8, 40)] {
                let count = rows * cols;
                let shapes = [
                    board(rows, cols, &styles(count, filled), view, center),
                    cube(cuboid_dims(count), &styles(count, filled), view, center),
                ]
                .concat();
                assert!(!shapes.is_empty());
                for s in shapes {
                    let coords = s.commands.split_whitespace().filter_map(|t| t.parse::<f64>().ok());
                    assert!(coords.into_iter().all(|c| (0.0..=CELL).contains(&c)), "{}", s.commands);
                }
            }
        }
    }

    #[test]
    fn full_cube_draws_only_outer_hull() {
        // A filled 2×2×2 cube viewed at an angle shows 3 sides with 4 squares each.
        let shapes = cube([2, 2, 2], &styles(8, 8), View::new(30.0, 30.0), Point { x: 120.0, y: 120.0 });
        assert_eq!(shapes.len(), 12);
    }
}
