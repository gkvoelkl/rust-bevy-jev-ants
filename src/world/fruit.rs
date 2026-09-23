//! Fruits: one cell each, an obstacle, and the thing the colony lives on.
//!
//! A fruit blocks its cell. An ant does not stand on a fruit, it picks it up
//! from a neighbouring cell — which makes it an object in the logic as much as
//! in the picture.

use bevy::prelude::*;
use rand::seq::IteratorRandom;

use crate::config::{CELL_SIZE, FRUIT_REGROWTH, FRUIT_TARGET};

use super::grid::{Grid, GridPos, Occupancy, Occupant};
use super::nest::Nest;
use super::render::Z_FRUIT;

const FRUIT: Color = Color::srgb(0.80, 0.26, 0.30);

#[derive(Component)]
pub struct Fruit;

/// Without regrowth the board is bare after fourteen fruits and the colony
/// stands around.
#[derive(Resource)]
pub struct Regrowth(Timer);

impl Default for Regrowth {
    fn default() -> Self {
        Self(Timer::from_seconds(FRUIT_REGROWTH, TimerMode::Repeating))
    }
}

pub fn plant_first_fruits(
    mut commands: Commands,
    grid: Res<Grid>,
    nest: Res<Nest>,
    mut occupancy: ResMut<Occupancy>,
    scenario: Option<Res<super::scenario::Scenario>>,
) {
    // On a task board the fruit lies where the board says, or the puzzle would
    // be different every time it is tried — and trying again is the point.
    if let Some(scenario) = scenario {
        for (x, y) in &scenario.fruit {
            plant_at(&mut commands, *grid, &mut occupancy, IVec2::new(*x, *y));
        }
        return;
    }
    for _ in 0..FRUIT_TARGET {
        plant(&mut commands, *grid, *nest, &mut occupancy);
    }
}

/// The ground a fruit needs: where the board is, where the nest is, and what
/// already stands on it.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Ground<'w> {
    grid: Res<'w, Grid>,
    nest: Res<'w, Nest>,
    occupancy: ResMut<'w, Occupancy>,
}

pub fn regrow(
    mut commands: Commands,
    time: Res<Time>,
    mut ground: Ground,
    mut regrowth: ResMut<Regrowth>,
    fruits: Query<&Fruit>,
    scenario: Option<Res<super::scenario::Scenario>>,
) {
    // A task board does not top itself up. What is there is what there is, and
    // that is what makes it a task.
    if scenario.is_some() {
        return;
    }
    if !regrowth.0.tick(time.delta()).just_finished() {
        return;
    }
    if fruits.iter().count() >= FRUIT_TARGET {
        return;
    }
    plant(
        &mut commands,
        *ground.grid,
        *ground.nest,
        &mut ground.occupancy,
    );
}

/// Puts one fruit on a free cell outside the nest. Nothing grows in the nest —
/// a fruit there would be delivered the moment it appeared.
fn plant(commands: &mut Commands, grid: Grid, nest: Nest, occupancy: &mut Occupancy) {
    let mut rng = rand::rng();
    let Some(cell) = grid
        .cells()
        .filter(|cell| !nest.contains(*cell) && occupancy.is_free(grid, *cell))
        .choose(&mut rng)
    else {
        return; // board full, which the target count makes unlikely
    };
    plant_at(commands, grid, occupancy, cell);
}

/// One fruit on a named cell. A cell already taken — or under water — is left
/// alone rather than overwritten.
fn plant_at(commands: &mut Commands, grid: Grid, occupancy: &mut Occupancy, cell: IVec2) {
    if !occupancy.is_free(grid, cell) {
        warn!("no room for a fruit at {cell:?}");
        return;
    }

    let fruit = commands
        .spawn((
            Name::new("fruit"),
            super::LevelEntity,
            Fruit,
            GridPos(cell),
            Sprite::from_color(FRUIT, Vec2::splat(CELL_SIZE * 0.46)),
            Transform::from_translation(grid.to_screen(cell).extend(Z_FRUIT)),
        ))
        .id();

    occupancy.occupy(grid, cell, Occupant::Fruit(fruit));
}
