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
) {
    for _ in 0..FRUIT_TARGET {
        plant(&mut commands, *grid, *nest, &mut occupancy);
    }
}

pub fn regrow(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<Grid>,
    nest: Res<Nest>,
    mut occupancy: ResMut<Occupancy>,
    mut regrowth: ResMut<Regrowth>,
    fruits: Query<&Fruit>,
) {
    if !regrowth.0.tick(time.delta()).just_finished() {
        return;
    }
    if fruits.iter().count() >= FRUIT_TARGET {
        return;
    }
    plant(&mut commands, *grid, *nest, &mut occupancy);
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

    let fruit = commands
        .spawn((
            Name::new("fruit"),
            Fruit,
            GridPos(cell),
            Sprite::from_color(FRUIT, Vec2::splat(CELL_SIZE * 0.46)),
            Transform::from_translation(grid.to_screen(cell).extend(Z_FRUIT)),
        ))
        .id();

    occupancy.occupy(grid, cell, Occupant::Fruit(fruit));
}
