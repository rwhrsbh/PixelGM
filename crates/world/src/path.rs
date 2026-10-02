//! Walking questions about a grid of cells: what can be reached, and what is nearest.

use crate::model::{MAP_H, MAP_W};
use std::collections::VecDeque;

pub type Cell = (usize, usize);

const STEPS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

fn neighbours((x, y): Cell) -> impl Iterator<Item = Cell> {
    STEPS.iter().filter_map(move |(dx, dy)| {
        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
        ((0..MAP_W as i32).contains(&nx) && (0..MAP_H as i32).contains(&ny)).then_some((nx as usize, ny as usize))
    })
}

/// Every cell that can be walked to from `start`, moving up, down, left and right.
pub fn reachable(start: Cell, blocked: impl Fn(Cell) -> bool) -> Vec<Vec<bool>> {
    let mut seen = vec![vec![false; MAP_W]; MAP_H];
    if blocked(start) {
        return seen;
    }
    seen[start.1][start.0] = true;
    let mut queue = VecDeque::from([start]);
    while let Some(cell) = queue.pop_front() {
        for next in neighbours(cell) {
            if !seen[next.1][next.0] && !blocked(next) {
                seen[next.1][next.0] = true;
                queue.push_back(next);
            }
        }
    }
    seen
}

/// The cell closest to `from` (itself included) for which `wanted` holds, walking through
/// anything. `None` only when no cell on the map qualifies.
pub fn nearest(from: Cell, wanted: impl Fn(Cell) -> bool) -> Option<Cell> {
    let mut seen = vec![vec![false; MAP_W]; MAP_H];
    seen[from.1][from.0] = true;
    let mut queue = VecDeque::from([from]);
    while let Some(cell) = queue.pop_front() {
        if wanted(cell) {
            return Some(cell);
        }
        for next in neighbours(cell) {
            if !seen[next.1][next.0] {
                seen[next.1][next.0] = true;
                queue.push_back(next);
            }
        }
    }
    None
}

/// The cells of an L-shaped walk from `a` to `b`: along the row first, then along the column.
/// Both ends are included.
pub fn elbow(a: Cell, b: Cell) -> Vec<Cell> {
    let mut cells = Vec::new();
    let mut x = a.0;
    loop {
        cells.push((x, a.1));
        if x == b.0 {
            break;
        }
        x = if b.0 > x { x + 1 } else { x - 1 };
    }
    let mut y = a.1;
    while y != b.1 {
        y = if b.1 > y { y + 1 } else { y - 1 };
        cells.push((b.0, y));
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wall_across_the_map_splits_it() {
        let wall = |(x, _): Cell| x == 10;
        let seen = reachable((2, 2), wall);
        assert!(seen[15][9] && seen[0][0]);
        assert!(!seen[2][10] && !seen[2][11]);
    }

    #[test]
    fn a_gap_in_the_wall_joins_it_again() {
        let wall = |(x, y): Cell| x == 10 && y != 7;
        assert!(reachable((2, 2), wall)[0][29]);
    }

    #[test]
    fn starting_inside_a_wall_reaches_nothing() {
        assert!(reachable((0, 0), |_| true).iter().flatten().all(|r| !r));
    }

    #[test]
    fn nearest_prefers_the_start_and_then_the_closest() {
        assert_eq!(nearest((5, 5), |_| true), Some((5, 5)));
        assert_eq!(nearest((5, 5), |(x, y)| x >= 8 && y == 5), Some((8, 5)));
        assert_eq!(nearest((5, 5), |_| false), None);
    }

    #[test]
    fn elbow_runs_along_the_row_then_the_column() {
        assert_eq!(elbow((1, 1), (3, 3)), [(1, 1), (2, 1), (3, 1), (3, 2), (3, 3)]);
        assert_eq!(elbow((3, 3), (1, 2)), [(3, 3), (2, 3), (1, 3), (1, 2)]);
        assert_eq!(elbow((4, 4), (4, 4)), [(4, 4)]);
    }
}
