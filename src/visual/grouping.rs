//! Splitting the elements of a shape into equally sized groups, so a fraction `n/d`
//! can be shown on a shape with a multiple of `d` elements.

/// One way of grouping elements: `group_of[element]` is the group index, and groups are
/// filled in index order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Partition {
    pub label: String,
    pub group_of: Vec<u32>,
}

impl Partition {
    /// Consecutive runs of `group_size` elements along `order` form one group each.
    pub fn from_order(label: impl Into<String>, order: &[usize], group_size: usize) -> Self {
        let mut group_of = vec![0; order.len()];
        for (k, &element) in order.iter().enumerate() {
            group_of[element] = (k / group_size) as u32;
        }
        Self { label: label.into(), group_of }
    }

    fn group_count(&self) -> usize {
        self.group_of.iter().max().map_or(0, |&g| g as usize + 1)
    }

    /// Whether every group holds exactly `group_size` elements.
    pub fn is_balanced(&self, group_size: usize) -> bool {
        let mut sizes = vec![0; self.group_count()];
        for &g in &self.group_of {
            sizes[g as usize] += 1;
        }
        sizes.iter().all(|&s| s == group_size)
    }
}

/// Drops partitions that group the elements exactly like an earlier one.
pub fn dedup(partitions: Vec<Partition>) -> Vec<Partition> {
    let mut unique: Vec<Partition> = Vec::new();
    for partition in partitions {
        if unique.iter().all(|u| u.group_of != partition.group_of) {
            unique.push(partition);
        }
    }
    unique
}

/// Greedy coloring of the groups so that neighbouring groups get different tints.
/// `adjacency` lists pairs of elements that touch.
pub fn group_tints(partition: &Partition, adjacency: &[(usize, usize)], tint_count: usize) -> Vec<usize> {
    let mut neighbors = vec![Vec::new(); partition.group_count()];
    for &(a, b) in adjacency {
        let (ga, gb) = (partition.group_of[a] as usize, partition.group_of[b] as usize);
        if ga != gb {
            neighbors[ga].push(gb);
            neighbors[gb].push(ga);
        }
    }

    let mut tints: Vec<Option<usize>> = vec![None; neighbors.len()];
    for group in 0..neighbors.len() {
        let used: Vec<usize> = neighbors[group].iter().filter_map(|&n| tints[n]).collect();
        let tint = (0..).find(|t| !used.contains(t)).unwrap_or(0);
        tints[group] = Some(tint % tint_count);
    }
    tints.into_iter().map(|t| t.unwrap_or(0)).collect()
}

/// Index of a grid cell in an n-dimensional grid with the first axis varying fastest.
pub fn grid_index(dims: &[u32], coords: &[u32]) -> usize {
    coords.iter().zip(dims).rev().fold(0, |acc, (&c, &d)| acc * d as usize + c as usize)
}

fn grid_coords(dims: &[u32], mut index: usize) -> Vec<u32> {
    dims.iter()
        .map(|&d| {
            let c = index % d as usize;
            index /= d as usize;
            c as u32
        })
        .collect()
}

fn grid_len(dims: &[u32]) -> usize {
    dims.iter().map(|&d| d as usize).product()
}

/// Pairs of cells sharing a face.
pub fn grid_adjacency(dims: &[u32]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for index in 0..grid_len(dims) {
        let coords = grid_coords(dims, index);
        for axis in 0..dims.len() {
            if coords[axis] + 1 < dims[axis] {
                let mut neighbor = coords.clone();
                neighbor[axis] += 1;
                pairs.push((index, grid_index(dims, &neighbor)));
            }
        }
    }
    pairs
}

/// Boustrophedon walk through the grid; `axes` lists the axes from fastest to slowest.
/// Consecutive cells always share a face, so every run of the walk is connected.
pub fn snake_order(dims: &[u32], axes: &[usize]) -> Vec<usize> {
    let mut path = vec![vec![0; dims.len()]];
    for &axis in axes {
        path = (0..dims[axis])
            .flat_map(|value| {
                let segment: Vec<Vec<u32>> = if value % 2 == 0 {
                    path.clone()
                } else {
                    path.iter().rev().cloned().collect()
                };
                segment.into_iter().map(move |mut coords| {
                    coords[axis] = value;
                    coords
                })
            })
            .collect();
    }
    path.iter().map(|coords| grid_index(dims, coords)).collect()
}

/// Every tiling of the grid by equal boxes of `group_size` cells, most compact boxes first.
/// Boxes are numbered in the grid's own index order.
pub fn block_partitions(dims: &[u32], group_size: u32) -> Vec<Partition> {
    fn block_shapes(dims: &[u32], remaining: u32) -> Vec<Vec<u32>> {
        match dims.split_first() {
            None if remaining == 1 => vec![Vec::new()],
            None => Vec::new(),
            Some((&first, rest)) => (1..=first)
                .filter(|b| first.is_multiple_of(*b) && remaining.is_multiple_of(*b))
                .flat_map(|b| {
                    block_shapes(rest, remaining / b).into_iter().map(move |mut tail| {
                        tail.insert(0, b);
                        tail
                    })
                })
                .collect(),
        }
    }

    let mut shapes = block_shapes(dims, group_size);
    shapes.sort_by_key(|block| block.iter().sum::<u32>());

    shapes
        .into_iter()
        .map(|block| {
            let counts: Vec<u32> = dims.iter().zip(&block).map(|(d, b)| d / b).collect();
            let group_of = (0..grid_len(dims))
                .map(|index| {
                    let coords = grid_coords(dims, index);
                    let block_coords: Vec<u32> = coords.iter().zip(&block).map(|(c, b)| c / b).collect();
                    grid_index(&counts, &block_coords) as u32
                })
                .collect();
            let size = block.iter().map(u32::to_string).collect::<Vec<_>>().join("×");
            Partition { label: format!("Blöcke {size}"), group_of }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_is_connected_and_complete() {
        let dims = [4, 3, 2];
        let order = snake_order(&dims, &[0, 1, 2]);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..24).collect::<Vec<_>>());

        let adjacency = grid_adjacency(&dims);
        for pair in order.windows(2) {
            let (a, b) = (pair[0].min(pair[1]), pair[0].max(pair[1]));
            assert!(adjacency.contains(&(a, b)), "{a} and {b} do not touch");
        }
    }

    #[test]
    fn blocks_tile_an_8x8_board() {
        let partitions = block_partitions(&[8, 8], 8);
        let labels: Vec<_> = partitions.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, ["Blöcke 2×4", "Blöcke 4×2", "Blöcke 1×8", "Blöcke 8×1"]);
        assert!(partitions.iter().all(|p| p.is_balanced(8)));
    }

    #[test]
    fn neighbouring_groups_get_different_tints() {
        let dims = [8, 8];
        let adjacency = grid_adjacency(&dims);
        for partition in block_partitions(&dims, 8) {
            let tints = group_tints(&partition, &adjacency, 4);
            for &(a, b) in &adjacency {
                let (ga, gb) = (partition.group_of[a] as usize, partition.group_of[b] as usize);
                assert!(ga == gb || tints[ga] != tints[gb]);
            }
        }
    }
}
