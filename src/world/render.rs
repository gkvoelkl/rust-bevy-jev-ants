//! Camera and the board itself. Nothing here knows about ants.

use bevy::prelude::*;

use crate::config::CELL_SIZE;

use super::grid::Grid;
use super::nest::Nest;
use super::scent::Scent;

pub const Z_BOARD: f32 = 0.0;
/// Above the ground and its scent, below anything that moves.
pub const Z_NEST_FRAME: f32 = 0.3;
pub const Z_FRUIT: f32 = 0.5;
pub const Z_ANT: f32 = 1.0;

const BACKGROUND: Color = Color::srgb(0.10, 0.09, 0.08);
const CELL_LIGHT: (f32, f32, f32) = (0.22, 0.20, 0.16);
const CELL_DARK: (f32, f32, f32) = (0.19, 0.17, 0.14);
const NEST_LIGHT: (f32, f32, f32) = (0.34, 0.26, 0.17);
const NEST_DARK: (f32, f32, f32) = (0.30, 0.23, 0.15);
/// What a cell shades towards as the scent on it grows. Muted on purpose — the
/// trails are ground, and the ants have to stay the brightest thing on screen.
const SCENT_TINT: (f32, f32, f32) = (0.28, 0.46, 0.30);
/// How far a cell at full strength shades towards it.
const SCENT_STRONGEST: f32 = 0.55;

/// Marks the nest out against the trails running through it.
const NEST_FRAME: Color = Color::srgb(0.90, 0.78, 0.30);
const FRAME_THICKNESS: f32 = 2.0;

/// A cell of the board, with the colour it has when nothing is on it.
#[derive(Component)]
pub struct BoardCell {
    cell: IVec2,
    base: (f32, f32, f32),
}

pub fn clear_color() -> ClearColor {
    ClearColor(BACKGROUND)
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// A checkerboard, so the grid is readable without drawing lines. The nest is
/// part of it rather than a separate object — it is ground, not a thing.
pub fn spawn_board(mut commands: Commands, grid: Res<Grid>, nest: Res<Nest>) {
    let grid = *grid;
    for cell in grid.cells() {
        let dark = (cell.x + cell.y) % 2 == 0;
        let color = match (nest.contains(cell), dark) {
            (true, true) => NEST_DARK,
            (true, false) => NEST_LIGHT,
            (false, true) => CELL_DARK,
            (false, false) => CELL_LIGHT,
        };
        commands.spawn((
            BoardCell { cell, base: color },
            Sprite::from_color(
                Color::srgb(color.0, color.1, color.2),
                Vec2::splat(CELL_SIZE),
            ),
            Transform::from_translation(grid.to_screen(cell).extend(Z_BOARD)),
        ));
    }
}

/// Paints the scent into the ground.
///
/// This is the best picture this demo has: a road forms between a fruit and the
/// nest that nobody planned.
pub fn show_scent(grid: Res<Grid>, scent: Res<Scent>, mut cells: Query<(&BoardCell, &mut Sprite)>) {
    let grid = *grid;
    for (board_cell, mut sprite) in &mut cells {
        let strength = scent.at(grid, board_cell.cell).clamp(0.0, 1.0);
        let towards = strength * SCENT_STRONGEST;
        let (red, green, blue) = board_cell.base;
        sprite.color = Color::srgb(
            red + (SCENT_TINT.0 - red) * towards,
            green + (SCENT_TINT.1 - green) * towards,
            blue + (SCENT_TINT.2 - blue) * towards,
        );
    }
}

/// A thin yellow outline around the nest.
///
/// The nest is ground like any other cell, so the scent tints it too and it
/// used to disappear into the trails converging on it. A frame keeps it
/// readable without making it look like an obstacle.
pub fn spawn_nest_frame(mut commands: Commands, grid: Res<Grid>, nest: Res<Nest>) {
    let grid = *grid;
    let first = grid.to_screen(nest.min);
    let last = grid.to_screen(nest.min + IVec2::splat(nest.size - 1));
    let centre = (first + last) * 0.5;
    let span = nest.size as f32 * CELL_SIZE;
    let half = span * 0.5;

    for (offset, size) in [
        (Vec2::new(0.0, half), Vec2::new(span, FRAME_THICKNESS)),
        (Vec2::new(0.0, -half), Vec2::new(span, FRAME_THICKNESS)),
        (Vec2::new(-half, 0.0), Vec2::new(FRAME_THICKNESS, span)),
        (Vec2::new(half, 0.0), Vec2::new(FRAME_THICKNESS, span)),
    ] {
        commands.spawn((
            Sprite::from_color(NEST_FRAME, size),
            Transform::from_translation((centre + offset).extend(Z_NEST_FRAME)),
        ));
    }
}
