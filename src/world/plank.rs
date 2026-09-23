//! The plank, and the river it goes across.
//!
//! One ant carries it. The puzzle is not the lifting but the **seeing**: no ant
//! knows there is fruit beyond the river, none knows the plank would help, and
//! none can see both at once — three cells of sight, and the two are half a
//! board apart. Only the queen sees the whole picture, and a sentence is all
//! she has to pass it on.
//!
//! Taking hold is one decision; the rest is execution (`ANTS.md` rule 3). The
//! ant walks the plank to the nearest crossing and lays it down by itself, over
//! as many seconds as that takes.

use bevy::prelude::*;

use crate::config::CELL_SIZE;

use super::grid::{Dir, Grid, Occupancy, Terrain};
use super::render::Z_FRUIT;

const PLANK: Color = Color::srgb(0.62, 0.45, 0.24);

/// The plank, and who has hold of it.
#[derive(Component, Default)]
pub struct Plank {
    /// `None` means it is lying on the ground, waiting for somebody.
    pub carrier: Option<Entity>,
    /// Where it is being taken, worked out when it is picked up.
    pub target: Option<IVec2>,
    /// Down across the water. From here it is terrain, not an object.
    pub laid: bool,
}

impl Plank {
    /// Whether an ant could still pick it up.
    pub fn free_to_take(&self) -> bool {
        !self.laid && self.carrier.is_none()
    }
}

/// A water cell worth bridging: one that really joins two banks.
///
/// Searched from where the plank stands, so the pair walks to the near side
/// rather than round the whole river.
pub fn nearest_crossing(grid: Grid, occupancy: &Occupancy, from: IVec2) -> Option<IVec2> {
    grid.cells()
        .filter(|cell| occupancy.terrain_at(grid, *cell) == Terrain::Water)
        .filter(|cell| {
            // Two passable neighbours means there is a bank on either side. A
            // cell walled in by more water would be a plank laid into nothing.
            Dir::COMPASS
                .into_iter()
                .filter(|direction| {
                    let beside = *cell + direction.offset();
                    grid.contains(beside) && occupancy.terrain_at(grid, beside).is_passable()
                })
                .count()
                >= 2
        })
        .min_by_key(|cell| {
            let offset = *cell - from;
            offset.x.abs().max(offset.y.abs())
        })
}

pub fn spawn_plank(
    mut commands: Commands,
    grid: Res<Grid>,
    mut occupancy: ResMut<Occupancy>,
    scenario: Option<Res<super::scenario::Scenario>>,
) {
    // No scenario, no plank: the open field is about fruit and pheromones.
    let Some(Some((x, y))) = scenario.map(|scenario| scenario.plank_at) else {
        return;
    };

    let grid = *grid;
    let cell = IVec2::new(x, y);
    let plank = commands
        .spawn((
            Name::new("plank"),
            super::LevelEntity,
            Plank::default(),
            super::grid::GridPos(cell),
            Sprite::from_color(PLANK, Vec2::new(CELL_SIZE * 0.85, CELL_SIZE * 0.3)),
            Transform::from_translation(grid.to_screen(cell).extend(Z_FRUIT)),
        ))
        .id();
    occupancy.occupy(grid, cell, super::grid::Occupant::Plank(plank));
}

/// A carried plank rides on the ant that has it. Purely cosmetic, but without
/// it the plank would vanish for as long as it travelled.
pub fn carried_plank_follows(
    mut planks: Query<(&Plank, &mut Transform, &mut Visibility), Without<super::grid::GridPos>>,
    carriers: Query<&Transform, With<super::grid::GridPos>>,
) {
    for (plank, mut transform, mut visibility) in &mut planks {
        let Some(on) = plank
            .carrier
            .and_then(|ant| carriers.get(ant).ok())
            .map(|ant| ant.translation.truncate())
        else {
            *visibility = Visibility::Hidden;
            continue;
        };
        *visibility = Visibility::Inherited;
        transform.translation = on.extend(Z_FRUIT + 0.05);
    }
}
