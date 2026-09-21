//! The board: cell coordinates, the mapping to the screen, and who stands where.
//!
//! Coordinates follow a map: `x` grows towards the east (right), `y` towards the
//! north (up). Bevy's screen `y` already points up, so the mapping is a plain
//! scale with an offset — no mirroring anywhere.

use bevy::prelude::*;

use crate::config::{CELL_SIZE, GRID_HEIGHT, GRID_WIDTH};

/// The eight compass directions an ant can step into, plus standing still.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
    Stay,
}

impl Dir {
    /// Every direction, including standing still.
    pub const ALL: [Dir; 9] = [
        Dir::North,
        Dir::NorthEast,
        Dir::East,
        Dir::SouthEast,
        Dir::South,
        Dir::SouthWest,
        Dir::West,
        Dir::NorthWest,
        Dir::Stay,
    ];

    /// The eight directions that actually move an ant, clockwise from north.
    pub const COMPASS: [Dir; 8] = [
        Dir::North,
        Dir::NorthEast,
        Dir::East,
        Dir::SouthEast,
        Dir::South,
        Dir::SouthWest,
        Dir::West,
        Dir::NorthWest,
    ];

    /// Cell offset of one step in this direction.
    pub fn offset(self) -> IVec2 {
        match self {
            Dir::North => IVec2::new(0, 1),
            Dir::NorthEast => IVec2::new(1, 1),
            Dir::East => IVec2::new(1, 0),
            Dir::SouthEast => IVec2::new(1, -1),
            Dir::South => IVec2::new(0, -1),
            Dir::SouthWest => IVec2::new(-1, -1),
            Dir::West => IVec2::new(-1, 0),
            Dir::NorthWest => IVec2::new(-1, 1),
            Dir::Stay => IVec2::ZERO,
        }
    }

    /// The key this direction gets in a `choice` question, via `Action::key`.
    pub fn key(self) -> &'static str {
        match self {
            Dir::North => "north",
            Dir::NorthEast => "north_east",
            Dir::East => "east",
            Dir::SouthEast => "south_east",
            Dir::South => "south",
            Dir::SouthWest => "south_west",
            Dir::West => "west",
            Dir::NorthWest => "north_west",
            Dir::Stay => "stay",
        }
    }

    /// How the direction is spelled in a sentence the model reads.
    pub fn spoken(self) -> &'static str {
        match self {
            Dir::North => "north",
            Dir::NorthEast => "north-east",
            Dir::East => "east",
            Dir::SouthEast => "south-east",
            Dir::South => "south",
            Dir::SouthWest => "south-west",
            Dir::West => "west",
            Dir::NorthWest => "north-west",
            Dir::Stay => "stay",
        }
    }

    /// The compass direction an offset points in, rounded to the nearest of the
    /// eight. An ant does not perceive exact angles either.
    pub fn nearest(offset: IVec2) -> Dir {
        if offset == IVec2::ZERO {
            return Dir::Stay;
        }
        let angle = (offset.y as f32).atan2(offset.x as f32);
        match (angle / std::f32::consts::FRAC_PI_4).round() as i32 % 8 {
            0 => Dir::East,
            1 | -7 => Dir::NorthEast,
            2 | -6 => Dir::North,
            3 | -5 => Dir::NorthWest,
            4 | -4 => Dir::West,
            5 | -3 => Dir::SouthWest,
            6 | -2 => Dir::South,
            _ => Dir::SouthEast,
        }
    }

    /// Rotation for a sprite whose body points north at angle zero.
    pub fn angle(self) -> f32 {
        let offset = self.offset();
        if offset == IVec2::ZERO {
            return 0.0;
        }
        (offset.y as f32).atan2(offset.x as f32) - std::f32::consts::FRAC_PI_2
    }
}

/// Dimensions of the board. Cheap to copy, so systems just take it by value.
#[derive(Resource, Clone, Copy)]
pub struct Grid {
    pub width: i32,
    pub height: i32,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            width: GRID_WIDTH,
            height: GRID_HEIGHT,
        }
    }
}

impl Grid {
    pub fn contains(&self, cell: IVec2) -> bool {
        cell.x >= 0 && cell.y >= 0 && cell.x < self.width && cell.y < self.height
    }

    /// Centre of the cell in world coordinates, board centred on the origin.
    pub fn to_screen(self, cell: IVec2) -> Vec2 {
        Vec2::new(
            (cell.x as f32 - (self.width - 1) as f32 * 0.5) * CELL_SIZE,
            (cell.y as f32 - (self.height - 1) as f32 * 0.5) * CELL_SIZE,
        )
    }

    fn index(&self, cell: IVec2) -> usize {
        (cell.y * self.width + cell.x) as usize
    }

    pub fn cells(&self) -> impl Iterator<Item = IVec2> + use<> {
        let (width, height) = (self.width, self.height);
        (0..height).flat_map(move |y| (0..width).map(move |x| IVec2::new(x, y)))
    }
}

/// A cell on the board. Ants carry it, and so does anything else that stands
/// on the grid.
#[derive(Component, Clone, Copy)]
pub struct GridPos(pub IVec2);

/// What stands on a cell. Both block movement — an ant walks around a fruit
/// just as it walks around another ant — but the ant is told them apart, so
/// "a fruit" never shows up in its view as "another ant".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Occupant {
    Ant(Entity),
    Fruit(Entity),
}

impl Occupant {
    pub fn entity(self) -> Entity {
        match self {
            Occupant::Ant(entity) | Occupant::Fruit(entity) => entity,
        }
    }
}

/// What stands on which cell. This is the single source of truth about
/// positions: nothing may ever enter a cell that is taken here.
#[derive(Resource)]
pub struct Occupancy {
    cells: Vec<Option<Occupant>>,
}

impl Occupancy {
    pub fn new(grid: Grid) -> Self {
        Self {
            cells: vec![None; (grid.width * grid.height) as usize],
        }
    }

    pub fn is_free(&self, grid: Grid, cell: IVec2) -> bool {
        grid.contains(cell) && self.cells[grid.index(cell)].is_none()
    }

    /// What is on this cell. Out of bounds reads as empty; the edge of the
    /// world is reported to the ant separately, because it is a different thing.
    pub fn at(&self, grid: Grid, cell: IVec2) -> Option<Occupant> {
        if !grid.contains(cell) {
            return None;
        }
        self.cells[grid.index(cell)]
    }

    pub fn occupy(&mut self, grid: Grid, cell: IVec2, occupant: Occupant) {
        self.cells[grid.index(cell)] = Some(occupant);
    }

    /// Frees a cell, but only if `who` is really what stands there. A stale
    /// call can then never evict somebody else.
    pub fn vacate(&mut self, grid: Grid, cell: IVec2, who: Entity) {
        let slot = &mut self.cells[grid.index(cell)];
        if slot.map(Occupant::entity) == Some(who) {
            *slot = None;
        }
    }
}

/// The directions this ant could step into right now — inside the board and
/// unoccupied. A blocked direction is never offered, which is what keeps Jev
/// from naming an impossible move. Standing still is not a direction; it is
/// `Action::Wait`, and `options::available` decides when it is worth offering.
pub fn free_directions(grid: Grid, occupancy: &Occupancy, from: IVec2) -> Vec<Dir> {
    Dir::COMPASS
        .into_iter()
        .filter(|dir| occupancy.is_free(grid, from + dir.offset()))
        .collect()
}
