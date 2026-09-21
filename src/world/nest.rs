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
        // Grid and nest both have even edges, so this division is exact and the
        // nest is centred to the cell.
        Self {
            min: IVec2::new((GRID_WIDTH - NEST_SIZE) / 2, (GRID_HEIGHT - NEST_SIZE) / 2),
            size: NEST_SIZE,
        }
    }
}

impl Nest {
    /// Every cell of the nest, north-west to south-east. The colony starts on
    /// these.
    pub fn cells(&self) -> impl Iterator<Item = IVec2> + use<> {
        let (min, size) = (self.min, self.size);
        (0..size).flat_map(move |dy| (0..size).map(move |dx| min + IVec2::new(dx, dy)))
    }

    /// The nest cell closest to `from`. What an ant walking home aims at, and
    /// what the distance in its view is measured against.
    pub fn nearest_cell(&self, from: IVec2) -> IVec2 {
        IVec2::new(
            from.x.clamp(self.min.x, self.min.x + self.size - 1),
            from.y.clamp(self.min.y, self.min.y + self.size - 1),
        )
    }

    /// Whether the nest is close enough to be seen from `from`. An ant that
    /// cannot see it does not know where it is — that is what the scent trail
    /// is for.
    pub fn within(&self, from: IVec2, radius: i32) -> bool {
        let offset = self.nearest_cell(from) - from;
        offset.x.abs().max(offset.y.abs()) <= radius
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
    use crate::config::{ANT_COUNT, NEST_SIZE};

    #[test]
    fn the_nest_sits_exactly_in_the_middle() {
        let nest = Nest::default();

        // Centre of the nest, in half-cells, against the centre of the board.
        let nest_centre = nest.min * 2 + IVec2::splat(nest.size - 1);
        let board_centre = IVec2::new(GRID_WIDTH - 1, GRID_HEIGHT - 1);
        assert_eq!(nest_centre, board_centre, "not centred");
    }

    #[test]
    fn it_has_room_for_the_whole_colony() {
        let nest = Nest::default();

        assert_eq!(nest.cells().count(), (NEST_SIZE * NEST_SIZE) as usize);
        assert!(
            nest.cells().count() >= ANT_COUNT,
            "every ant has to start somewhere inside"
        );
        assert!(nest.cells().all(|cell| nest.contains(cell)));
    }

    #[test]
    fn just_outside_is_outside() {
        let nest = Nest::default();
        assert!(!nest.contains(nest.min - IVec2::X));
        assert!(!nest.contains(nest.min - IVec2::Y));
        assert!(!nest.contains(nest.min + IVec2::splat(nest.size)));
    }

    #[test]
    fn it_is_only_in_sight_from_nearby() {
        let nest = Nest::default();

        assert!(nest.within(nest.min, 3), "standing in it");
        assert!(nest.within(nest.min - IVec2::splat(2), 3));
        assert!(!nest.within(IVec2::ZERO, 3), "from the corner of the board");
    }

    #[test]
    fn from_inside_the_nearest_cell_is_where_the_ant_stands() {
        let nest = Nest::default();
        assert_eq!(nest.nearest_cell(nest.min), nest.min);
    }

    #[test]
    fn from_outside_the_nearest_cell_faces_the_ant() {
        let nest = Nest::default();
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
