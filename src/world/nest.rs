//! The nest: a square in the middle of the board, and what the colony has
//! brought home.

use bevy::prelude::*;

use crate::config::{GRID_HEIGHT, GRID_WIDTH, NEST_SIZE};

/// Walkable on purpose — an ant enters it to put a fruit down.
#[derive(Resource, Clone, Copy)]
pub struct Nest {
    pub min: IVec2,
    pub size: i32,
}

impl Default for Nest {
    fn default() -> Self {
        // Both grid sides are odd, so the centre is a cell and the nest is
        // exactly centred on it.
        Self {
            min: IVec2::new((GRID_WIDTH - NEST_SIZE) / 2, (GRID_HEIGHT - NEST_SIZE) / 2),
            size: NEST_SIZE,
        }
    }
}

impl Nest {
    /// The nest cell closest to `from`. What an ant walking home aims at, and
    /// what the distance in its view is measured against.
    pub fn nearest_cell(&self, from: IVec2) -> IVec2 {
        IVec2::new(
            from.x.clamp(self.min.x, self.min.x + self.size - 1),
            from.y.clamp(self.min.y, self.min.y + self.size - 1),
        )
    }

    pub fn contains(&self, cell: IVec2) -> bool {
        cell.x >= self.min.x
            && cell.y >= self.min.y
            && cell.x < self.min.x + self.size
            && cell.y < self.min.y + self.size
    }
}

/// Fruits delivered so far. Losing the game will later mean this running out,
/// which needs consumption as well as growth.
#[derive(Resource, Default)]
pub struct Stores(pub u32);

#[cfg(test)]
mod tests {
    use super::*;

    fn centre() -> IVec2 {
        IVec2::new(GRID_WIDTH / 2, GRID_HEIGHT / 2)
    }

    #[test]
    fn the_nest_sits_in_the_middle() {
        let nest = Nest::default();

        assert!(nest.contains(centre()));
        // One ring of nest around the centre cell, and nothing beyond it.
        assert_eq!(nest.min, centre() - IVec2::ONE);
        assert!(nest.contains(centre() + IVec2::ONE));
        assert!(!nest.contains(centre() + IVec2::splat(2)));
    }

    #[test]
    fn the_nearest_cell_faces_the_ant() {
        let nest = Nest::default();
        assert_eq!(nest.nearest_cell(centre()), centre());

        let far_east = IVec2::new(GRID_WIDTH - 1, GRID_HEIGHT / 2);
        let target = nest.nearest_cell(far_east);
        assert!(nest.contains(target));
        assert_eq!(target.x, nest.min.x + nest.size - 1, "its eastern edge");
    }

    #[test]
    fn the_corners_of_the_board_are_not_the_nest() {
        let nest = Nest::default();
        for corner in [
            IVec2::ZERO,
            IVec2::new(GRID_WIDTH - 1, 0),
            IVec2::new(0, GRID_HEIGHT - 1),
            IVec2::new(GRID_WIDTH - 1, GRID_HEIGHT - 1),
        ] {
            assert!(!nest.contains(corner), "{corner:?}");
        }
    }
}
