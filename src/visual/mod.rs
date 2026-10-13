//! Turns a fraction into flat `Path` polygons that the Slint canvas draws in order.
//!
//! Every shape consists of a number of equal *elements* (sectors, tiles, small triangles,
//! small cubes). For a denominator `d` the elements are split into `d` equal *groups*,
//! and the first `numerator` groups are highlighted.

mod flat;
mod grouping;
mod solid;

pub use solid::View;

use std::fmt::{self, Write};

use slint::Color;

use crate::fraction::MixedFraction;
use crate::{Shape, VisualMode};
use flat::TriangleGrid;
use grouping::Partition;

/// Side length (in viewbox units) of the square cell each unit is drawn in.
pub const CELL: f64 = 240.0;

const MAX_PIE_SECTORS: u32 = 240;
const MAX_TRIANGLES: u32 = 200;
const MAX_BOARD_SIDE: u32 = 16;
const MAX_CUBES: u32 = 216;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// How many elements the user asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Elements {
    /// Pick the smallest sensible number for the denominator.
    Auto,
    Count(u32),
    Grid { rows: u32, cols: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    NotMultiple { count: u32, denominator: u32 },
    TooMany { max: u32 },
    BoardTooLarge,
    NotTriangular { count: u32, suggestions: Vec<u32> },
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotMultiple { count, denominator } => {
                write!(f, "Die Anzahl Teile ({count}) muss ein Vielfaches des Nenners ({denominator}) sein.")
            }
            Self::TooMany { max } => write!(f, "Es sind höchstens {max} Teile möglich."),
            Self::BoardTooLarge => write!(f, "Das Brett darf höchstens {MAX_BOARD_SIDE} × {MAX_BOARD_SIDE} Felder haben."),
            Self::NotTriangular { count, suggestions } => {
                write!(f, "Aus {count} gleichschenkligen Dreiecken lässt sich kein grosses Dreieck legen.")?;
                if !suggestions.is_empty() {
                    let list: Vec<String> = suggestions.iter().map(u32::to_string).collect();
                    write!(f, "\nMögliche Anzahlen: {}", list.join(", "))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for LayoutError {}

/// Everything the canvas needs: shapes in back-to-front order plus the viewbox size,
/// and the available group sizes and groupings to choose from.
#[derive(Default)]
pub struct Scene {
    pub shapes: Vec<Shape>,
    pub width: f64,
    pub height: f64,
    pub group_choice: usize,
    pub group_labels: Vec<String>,
    pub variant: usize,
    pub variant_labels: Vec<String>,
    pub summary: String,
    /// Whether the elements form groups of more than one element.
    pub grouped: bool,
}

/// Which group decorations to draw (only relevant when groups hold several elements).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupDisplay {
    /// Alternating shades for neighbouring groups.
    pub shading: bool,
    /// An outline around every group.
    pub outline: bool,
}

/// The user's picks; out-of-range values wrap around.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    /// Index into the possible group counts; `None` picks the entered denominator.
    pub group_choice: Option<i32>,
    /// Index into the groupings available for the chosen group count.
    pub variant: i32,
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Every number of groups that shows the fraction exactly on `count` elements: the
/// denominators of all equivalent fractions that divide `count`, largest groups first.
fn group_counts(fraction: MixedFraction, count: u32) -> Vec<u32> {
    let reduced = fraction.denominator / gcd(fraction.numerator, fraction.denominator);
    (1..=count).filter(|&groups| groups.is_multiple_of(reduced) && count.is_multiple_of(groups)).collect()
}

/// Draws one shape per unit of `fraction` side by side, split into the selected number of
/// groups and arranged by the selected grouping.
pub fn build(
    mode: VisualMode,
    fraction: Option<MixedFraction>,
    elements: Elements,
    selection: Selection,
    display: GroupDisplay,
    view: View,
) -> Result<Scene, LayoutError> {
    let Some(fraction) = fraction else { return Ok(Scene::default()) };
    let entered = fraction.denominator;
    let geometry = Geometry::new(mode, elements, entered)?;
    let count = geometry.element_count();

    // `entered` divides `count` (checked by `Geometry::new`), so it is always an option.
    let counts = group_counts(fraction, count);
    let group_choice = match selection.group_choice {
        Some(choice) => choice.rem_euclid(counts.len() as i32) as usize,
        None => counts.iter().position(|&g| g == entered).unwrap_or(0),
    };
    let groups = counts[group_choice];
    let group_size = count / groups;

    let partitions = match (groups, group_size) {
        (1, _) | (_, 1) => {
            let label = if groups == 1 { "Ein Ganzes" } else { "Einzelne Teile" };
            let identity: Vec<usize> = (0..count as usize).collect();
            vec![Partition::from_order(label, &identity, group_size as usize)]
        }
        _ => grouping::dedup(geometry.partitions(group_size)),
    };
    let variant = selection.variant.rem_euclid(partitions.len() as i32) as usize;
    let partition = &partitions[variant];

    let grouped = group_size > 1 && groups > 1;
    let tints = if grouped && display.shading {
        grouping::group_tints(partition, &geometry.adjacency(), TINTS)
    } else {
        vec![0; groups as usize]
    };

    let units = fraction.units();
    let shapes = units
        .iter()
        .enumerate()
        .flat_map(|(i, unit)| {
            // Exact: `groups` is a multiple of the reduced denominator.
            let filled = unit.filled * groups / entered;
            let styles: Vec<Style> = partition
                .group_of
                .iter()
                .map(|&group| Style {
                    filled: group < filled,
                    tint: tints[group as usize],
                    group: (grouped && display.outline).then_some(group),
                })
                .collect();
            let center = Point { x: CELL * (i as f64 + 0.5), y: CELL / 2.0 };
            geometry.render(&styles, view, center)
        })
        .collect();

    let group_labels = counts
        .iter()
        .map(|&g| {
            let size = count / g;
            let groups_text = if g == 1 { "1 Gruppe".to_string() } else { format!("{g} Gruppen") };
            match fraction.numerator {
                0 => format!("{groups_text} à {size}"),
                n => format!("{}/{g} · {groups_text} à {size}", n * g / entered),
            }
        })
        .collect();

    let summary = match group_size {
        1 => format!("{count} Teile"),
        _ => format!("{count} Teile · {groups} Gruppen à {group_size} Teile"),
    };

    Ok(Scene {
        shapes,
        width: CELL * units.len() as f64,
        height: CELL,
        group_choice,
        group_labels,
        variant,
        variant_labels: partitions.into_iter().map(|p| p.label).collect(),
        summary,
        grouped,
    })
}

/// The element structure of one shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Geometry {
    Pie(u32),
    Triangle(TriangleGrid),
    Board { rows: u32, cols: u32 },
    Cube([u32; 3]),
}

impl Geometry {
    fn new(mode: VisualMode, elements: Elements, denominator: u32) -> Result<Self, LayoutError> {
        let check = |count: u32, max: u32| match count {
            c if !c.is_multiple_of(denominator) => Err(LayoutError::NotMultiple { count, denominator }),
            c if c > max => Err(LayoutError::TooMany { max }),
            c => Ok(c),
        };
        let count = match elements {
            Elements::Auto => None,
            Elements::Count(c) => Some(c),
            Elements::Grid { rows, cols } => Some(rows * cols),
        };

        match mode {
            VisualMode::Pie => Ok(Self::Pie(check(count.unwrap_or(denominator), MAX_PIE_SECTORS)?)),
            VisualMode::Cube => Ok(Self::Cube(solid::cuboid_dims(check(count.unwrap_or(denominator), MAX_CUBES)?))),
            VisualMode::Board => {
                let (rows, cols) = match elements {
                    Elements::Grid { rows, cols } => (rows, cols),
                    _ => solid::board_dims(denominator),
                };
                if rows > MAX_BOARD_SIDE || cols > MAX_BOARD_SIDE {
                    return Err(LayoutError::BoardTooLarge);
                }
                check(rows * cols, MAX_BOARD_SIDE * MAX_BOARD_SIDE)?;
                Ok(Self::Board { rows, cols })
            }
            VisualMode::Triangle => {
                let multiples = (1..).map(|m| m * denominator).take_while(|&c| c <= MAX_TRIANGLES);
                let count = match count {
                    Some(c) => check(c, MAX_TRIANGLES)?,
                    // A square of the denominator always works.
                    None => multiples.clone().find(|&c| TriangleGrid::for_count(c).is_some()).unwrap_or(denominator * denominator),
                };
                TriangleGrid::for_count(count).map(Self::Triangle).ok_or_else(|| LayoutError::NotTriangular {
                    count,
                    suggestions: multiples.filter(|&c| TriangleGrid::for_count(c).is_some()).take(8).collect(),
                })
            }
        }
    }

    fn element_count(&self) -> u32 {
        match *self {
            Self::Pie(n) => n,
            Self::Triangle(grid) => grid.count(),
            Self::Board { rows, cols } => rows * cols,
            Self::Cube(dims) => dims.iter().product(),
        }
    }

    /// Pairs of elements sharing an edge (2D) or a face (3D).
    fn adjacency(&self) -> Vec<(usize, usize)> {
        match *self {
            Self::Pie(n) => {
                let n = n as usize;
                (0..n).map(|i| (i, (i + 1) % n)).collect()
            }
            Self::Triangle(grid) => grid.adjacency(),
            Self::Board { rows, cols } => grouping::grid_adjacency(&[cols, rows]),
            Self::Cube(dims) => grouping::grid_adjacency(&dims),
        }
    }

    /// The candidate groupings into groups of `group_size` elements.
    fn partitions(&self, group_size: u32) -> Vec<Partition> {
        let size = group_size as usize;
        match *self {
            Self::Pie(n) => {
                let order: Vec<usize> = (0..n as usize).collect();
                vec![Partition::from_order("Zusammenhängende Sektoren", &order, size)]
            }
            Self::Triangle(grid) => grid.partitions(group_size),
            Self::Board { rows, cols } => {
                // Tiles are indexed row by row from the top, i.e. x = column, y = row.
                let dims = [cols, rows];
                let mut partitions = grouping::block_partitions(&dims, group_size);
                partitions.push(Partition::from_order("Zeilenweise (Schlange)", &grouping::snake_order(&dims, &[0, 1]), size));
                partitions.push(Partition::from_order("Spaltenweise (Schlange)", &grouping::snake_order(&dims, &[1, 0]), size));
                partitions
            }
            Self::Cube(dims) => {
                let mut partitions = grouping::block_partitions(&dims, group_size);
                partitions.push(Partition::from_order("Schicht für Schicht", &grouping::snake_order(&dims, &[0, 1, 2]), size));
                partitions.push(Partition::from_order("Scheibe für Scheibe", &grouping::snake_order(&dims, &[2, 0, 1]), size));
                partitions
            }
        }
    }

    fn render(&self, styles: &[Style], view: View, center: Point) -> Vec<Shape> {
        match *self {
            Self::Pie(n) => flat::pie(n, styles, center),
            Self::Triangle(grid) => flat::triangle(grid, styles, center),
            Self::Board { rows, cols } => solid::board(rows, cols, styles, view, center),
            Self::Cube(dims) => solid::cube(dims, styles, view, center),
        }
    }
}

/// How a single element is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Style {
    filled: bool,
    /// Alternating shade, so neighbouring groups can be told apart.
    tint: usize,
    /// The element's group, when groups are outlined (i.e. hold more than one element).
    group: Option<u32>,
}

impl Style {
    fn color(self) -> Rgb {
        let palette = if self.filled { &ACTIVE_TINTS } else { &INACTIVE_TINTS };
        palette[self.tint % TINTS]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb(u8, u8, u8);

impl Rgb {
    fn shaded(self, factor: f64, alpha: u8) -> Color {
        let scale = |c: u8| (f64::from(c) * factor).round().clamp(0.0, 255.0) as u8;
        Color::from_argb_u8(alpha, scale(self.0), scale(self.1), scale(self.2))
    }

    fn opaque(self) -> Color {
        self.shaded(1.0, 255)
    }
}

const TINTS: usize = 4;
const ACTIVE_TINTS: [Rgb; TINTS] = [Rgb(59, 130, 246), Rgb(29, 78, 216), Rgb(96, 165, 250), Rgb(30, 64, 175)];
const INACTIVE_TINTS: [Rgb; TINTS] = [Rgb(226, 232, 240), Rgb(186, 198, 214), Rgb(241, 245, 249), Rgb(148, 163, 184)];

/// SVG-style path commands for a closed polygon.
fn polygon(points: &[Point]) -> String {
    let mut commands = String::new();
    for (i, p) in points.iter().enumerate() {
        let op = if i == 0 { 'M' } else { 'L' };
        // Writing into a String cannot fail.
        let _ = write!(commands, "{op} {:.2} {:.2} ", p.x, p.y);
    }
    commands.push('Z');
    commands
}

fn shape(commands: String, fill: Color, stroke: Color, stroke_width: f32) -> Shape {
    Shape { commands: commands.into(), fill, stroke, stroke_width }
}

/// Color of the outline drawn around each group of elements.
const OUTLINE: Rgb = Rgb(249, 115, 22);

/// Path commands for a set of separate line segments.
fn segments(lines: impl IntoIterator<Item = [Point; 2]>) -> String {
    let mut commands = String::new();
    for [a, b] in lines {
        // Writing into a String cannot fail.
        let _ = write!(commands, "M {:.2} {:.2} L {:.2} {:.2} ", a.x, a.y, b.x, b.y);
    }
    commands
}

/// An unfilled stroke in the group outline color.
fn outline(commands: String, stroke_width: f32) -> Shape {
    shape(commands, Color::from_argb_u8(0, 0, 0, 0), OUTLINE.opaque(), stroke_width)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: GroupDisplay = GroupDisplay { shading: true, outline: true };
    const ENTERED: Selection = Selection { group_choice: None, variant: 0 };

    fn fraction(numerator: u32, denominator: u32) -> Option<MixedFraction> {
        Some(MixedFraction { wholes: 0, numerator, denominator })
    }

    #[test]
    fn every_partition_has_denominator_equal_groups() {
        let cases = [
            (VisualMode::Pie, Elements::Count(24), 8),
            (VisualMode::Board, Elements::Grid { rows: 8, cols: 8 }, 8),
            (VisualMode::Board, Elements::Grid { rows: 4, cols: 5 }, 4),
            (VisualMode::Triangle, Elements::Count(64), 4),
            (VisualMode::Triangle, Elements::Count(32), 8),
            (VisualMode::Triangle, Elements::Count(36), 9),
            (VisualMode::Cube, Elements::Count(64), 8),
        ];
        for (mode, elements, denominator) in cases {
            let geometry = Geometry::new(mode, elements, denominator).unwrap();
            let group_size = geometry.element_count() / denominator;
            let partitions = geometry.partitions(group_size);
            assert!(!partitions.is_empty(), "{mode:?}");
            for p in partitions {
                assert!(p.is_balanced(group_size as usize), "{mode:?} {}", p.label);
                assert_eq!(p.group_of.iter().max(), Some(&(denominator - 1)));
            }
        }
    }

    #[test]
    fn five_eighths_on_an_8x8_board() {
        let elements = Elements::Grid { rows: 8, cols: 8 };
        let scene = build(VisualMode::Board, fraction(5, 8), elements, ENTERED, ALL, View::new(25.0, 35.0)).unwrap();
        assert_eq!(scene.variant_labels.len(), 4);
        assert_eq!(scene.summary, "64 Teile · 8 Gruppen à 8 Teile");

        let wrapped = build(VisualMode::Board, fraction(5, 8), elements, Selection { group_choice: None, variant: -1 }, ALL, View::new(25.0, 35.0)).unwrap();
        assert_eq!(wrapped.variant, 3);
    }

    #[test]
    fn groups_are_outlined_only_when_they_hold_several_elements() {
        let outlines = |mode, elements| {
            let scene = build(mode, fraction(3, 4), elements, ENTERED, ALL, View::new(25.0, 35.0)).unwrap();
            scene.shapes.iter().filter(|s| s.stroke == OUTLINE.opaque()).count()
        };
        for mode in [VisualMode::Pie, VisualMode::Triangle, VisualMode::Board, VisualMode::Cube] {
            assert_eq!(outlines(mode, Elements::Auto), 0, "{mode:?}");
        }
        assert_eq!(outlines(VisualMode::Pie, Elements::Count(16)), 1);
        assert_eq!(outlines(VisualMode::Triangle, Elements::Count(16)), 1);
        assert_eq!(outlines(VisualMode::Board, Elements::Grid { rows: 4, cols: 4 }), 1);
        // Cubes draw outlines per cube, interleaved with the faces.
        assert!(outlines(VisualMode::Cube, Elements::Count(16)) > 1);
    }

    #[test]
    fn group_decorations_can_be_switched_off() {
        let elements = Elements::Grid { rows: 4, cols: 4 };
        let render = |display| build(VisualMode::Board, fraction(3, 4), elements, ENTERED, display, View::new(25.0, 35.0)).unwrap();
        let colors = |scene: &Scene| {
            let mut fills: Vec<_> = scene.shapes.iter().map(|s| s.fill).collect();
            fills.sort_by_key(|c| c.as_argb_encoded());
            fills.dedup();
            fills.len()
        };
        let has_outline = |scene: &Scene| scene.shapes.iter().any(|s| s.stroke == OUTLINE.opaque());

        let plain = render(GroupDisplay { shading: false, outline: false });
        let outlined = render(GroupDisplay { shading: false, outline: true });
        let shaded = render(GroupDisplay { shading: true, outline: false });
        assert!(plain.grouped && !has_outline(&plain) && has_outline(&outlined) && !has_outline(&shaded));
        assert!(colors(&shaded) > colors(&plain));
    }

    #[test]
    fn all_equivalent_group_counts_are_offered() {
        let board = Elements::Grid { rows: 8, cols: 8 };
        let scene = |n, d, choice| {
            let selection = Selection { group_choice: choice, variant: 0 };
            build(VisualMode::Board, fraction(n, d), board, selection, ALL, View::new(25.0, 35.0)).unwrap()
        };

        let five_eighths = scene(5, 8, None);
        assert_eq!(
            five_eighths.group_labels,
            ["5/8 · 8 Gruppen à 8", "10/16 · 16 Gruppen à 4", "20/32 · 32 Gruppen à 2", "40/64 · 64 Gruppen à 1"]
        );
        assert_eq!(five_eighths.group_choice, 0);

        // 4/8 reduces to 1/2, so two groups of 32 are the largest possible ones.
        let half = scene(4, 8, None);
        assert_eq!(half.group_labels.len(), 6);
        assert_eq!(half.group_labels[0], "1/2 · 2 Gruppen à 32");
        assert_eq!(half.group_choice, 2);

        let largest = scene(4, 8, Some(0));
        assert_eq!(largest.summary, "64 Teile · 2 Gruppen à 32 Teile");
        assert_eq!(largest.variant_labels[0], "Blöcke 4×8");

        // Single elements: no grouping decorations.
        assert!(!scene(4, 8, Some(5)).grouped);
    }

    #[test]
    fn element_count_must_fit_denominator() {
        assert_eq!(
            Geometry::new(VisualMode::Pie, Elements::Count(10), 4),
            Err(LayoutError::NotMultiple { count: 10, denominator: 4 })
        );
        assert_eq!(
            Geometry::new(VisualMode::Board, Elements::Grid { rows: 5, cols: 4 }, 3),
            Err(LayoutError::NotMultiple { count: 20, denominator: 3 })
        );
        match Geometry::new(VisualMode::Triangle, Elements::Count(12), 3) {
            Err(LayoutError::NotTriangular { suggestions, .. }) => assert_eq!(suggestions, [9, 18, 36, 72, 81, 144, 162]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn auto_triangle_count_is_smallest_valid_multiple() {
        let count = |d| Geometry::new(VisualMode::Triangle, Elements::Auto, d).unwrap().element_count();
        assert_eq!([count(1), count(2), count(3), count(5), count(8)], [1, 2, 9, 25, 8]);
    }
}
