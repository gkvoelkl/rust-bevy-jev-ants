//! Camera and the board itself. Nothing here knows about ants.

use bevy::prelude::*;

use crate::config::CELL_SIZE;

use super::grid::Grid;

pub const Z_BOARD: f32 = 0.0;
pub const Z_ANT: f32 = 1.0;

const BACKGROUND: Color = Color::srgb(0.10, 0.09, 0.08);
const CELL_LIGHT: Color = Color::srgb(0.22, 0.20, 0.16);
const CELL_DARK: Color = Color::srgb(0.19, 0.17, 0.14);

pub fn clear_color() -> ClearColor {
    ClearColor(BACKGROUND)
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// A checkerboard, so the grid is readable without drawing lines.
pub fn spawn_board(mut commands: Commands, grid: Res<Grid>) {
    let grid = *grid;
    for cell in grid.cells() {
        let color = if (cell.x + cell.y) % 2 == 0 {
            CELL_DARK
        } else {
            CELL_LIGHT
        };
        commands.spawn((
            Sprite::from_color(color, Vec2::splat(CELL_SIZE)),
            Transform::from_translation(grid.to_screen(cell).extend(Z_BOARD)),
        ));
    }
}
