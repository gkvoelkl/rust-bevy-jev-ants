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
/// The river. Cold against the warm ground, so "you cannot go there" reads
/// before anybody has explained it.
const WATER_LIGHT: (f32, f32, f32) = (0.16, 0.30, 0.46);
const WATER_DARK: (f32, f32, f32) = (0.13, 0.26, 0.41);
/// The plank once it is down: ground again, but visibly made rather than grown.
pub const BRIDGE: (f32, f32, f32) = (0.55, 0.40, 0.22);
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

/// How the board sits on the screen right now.
///
/// One place for the mapping between world and screen, because three things
/// need it and they must agree: the camera, the labels drawn over each ant, and
/// the click that picks one. Before there was any zoom all three quietly
/// assumed a scale of 1 and a camera at the origin, and each did the arithmetic
/// itself.
///
/// Written by the UI, which is the only part that knows how much room the
/// panels have left over, and read by everything else.
#[derive(Resource)]
pub struct View {
    /// World units per logical pixel. Below 1 is zoomed in.
    pub scale: f32,
    /// Where the world origin sits on screen, in egui coordinates: the middle
    /// of whatever the panels left free.
    pub origin: Vec2,
    /// The middle of the whole window, which is where a camera at the origin
    /// points. The two differ because the bar above is taller than the one
    /// below, and that difference is exactly what the camera has to make up.
    pub window_centre: Vec2,
}

impl Default for View {
    fn default() -> Self {
        Self {
            scale: 1.0,
            origin: Vec2::ZERO,
            window_centre: Vec2::ZERO,
        }
    }
}

/// Air between the board and the panels, in logical pixels.
///
/// Wide enough to write in: the four compass words sit in this gap, outside the
/// board on every side (`ui::hud::compass`). Every board keeps at least this
/// much on each edge, whichever axis the fit is decided on.
pub const BOARD_MARGIN: f32 = 26.0;

impl View {
    /// The scale at which `grid` fills `area` without spilling out of it.
    pub fn fit(grid: Grid, area: Vec2) -> f32 {
        let board = Vec2::new(grid.width as f32, grid.height as f32) * CELL_SIZE;
        let room = (area - Vec2::splat(BOARD_MARGIN * 2.0)).max(Vec2::splat(1.0));
        (board.x / room.x).max(board.y / room.y).max(f32::EPSILON)
    }

    pub fn to_screen(&self, world: Vec2) -> Vec2 {
        self.origin + Vec2::new(world.x, -world.y) / self.scale
    }

    pub fn to_world(&self, screen: Vec2) -> Vec2 {
        let offset = screen - self.origin;
        Vec2::new(offset.x, -offset.y) * self.scale
    }
}

/// Points the camera at the board so it fills whatever the panels leave.
///
/// A level may be a quarter the size of the last one — level 1 is 24 x 16 where
/// the open field is 64 x 32 — and a board floating in the middle of a large
/// window is hard to read for no reason.
pub fn fit_camera(
    view: Res<View>,
    mut camera: Query<(&mut Projection, &mut Transform), With<Camera2d>>,
) {
    for (mut projection, mut transform) in &mut camera {
        if let Projection::Orthographic(orthographic) = &mut *projection {
            orthographic.scale = view.scale;
        }
        // Where the world origin should land, measured from where a camera at
        // the origin would put it. Screen y grows downward and world y upward,
        // which is the whole reason for the sign flip.
        let offset = view.origin - view.window_centre;
        transform.translation.x = -view.scale * offset.x;
        transform.translation.y = view.scale * offset.y;
    }
}

pub fn clear_color() -> ClearColor {
    ClearColor(BACKGROUND)
}

pub fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// A checkerboard, so the grid is readable without drawing lines. The nest is
/// part of it rather than a separate object — it is ground, not a thing.
pub fn spawn_board(
    mut commands: Commands,
    grid: Res<Grid>,
    nest: Res<Nest>,
    occupancy: Res<super::grid::Occupancy>,
) {
    let grid = *grid;
    for cell in grid.cells() {
        let dark = (cell.x + cell.y) % 2 == 0;
        let water = occupancy.terrain_at(grid, cell) == super::grid::Terrain::Water;
        let color = match (water, nest.contains(cell), dark) {
            (true, _, true) => WATER_DARK,
            (true, _, false) => WATER_LIGHT,
            (false, true, true) => NEST_DARK,
            (false, true, false) => NEST_LIGHT,
            (false, false, true) => CELL_DARK,
            (false, false, false) => CELL_LIGHT,
        };
        commands.spawn((
            super::LevelEntity,
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
pub fn show_scent(
    grid: Res<Grid>,
    scent: Res<Scent>,
    occupancy: Res<super::grid::Occupancy>,
    mut cells: Query<(&BoardCell, &mut Sprite)>,
) {
    let grid = *grid;
    for (board_cell, mut sprite) in &mut cells {
        // A cell that was water and now carries the plank changes colour once
        // and stays. Scent on a bridge would only confuse the picture.
        if occupancy.terrain_at(grid, board_cell.cell) == super::grid::Terrain::Bridge {
            sprite.color = Color::srgb(BRIDGE.0, BRIDGE.1, BRIDGE.2);
            continue;
        }
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
            super::LevelEntity,
            Sprite::from_color(NEST_FRAME, size),
            Transform::from_translation((centre + offset).extend(Z_NEST_FRAME)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stretch of window with the two bars taken off it.
    const FREE: Vec2 = Vec2::new(1276.0, 646.0);

    fn board_size(grid: Grid) -> Vec2 {
        Vec2::new(grid.width as f32, grid.height as f32) * CELL_SIZE
    }

    /// Whatever the level, the whole board has to be on screen — that is the
    /// entire job, and off-by-one in the wrong direction means a row of cells
    /// hidden under a panel.
    #[test]
    fn every_board_fits_the_space_it_is_given() {
        for grid in [
            Grid::default(),
            Grid {
                width: 24,
                height: 16,
            },
            Grid {
                width: 30,
                height: 20,
            },
            // Absurdly wide and absurdly tall, to catch a fit that only ever
            // looked at one axis.
            Grid {
                width: 200,
                height: 4,
            },
            Grid {
                width: 4,
                height: 200,
            },
        ] {
            let scale = View::fit(grid, FREE);
            let on_screen = board_size(grid) / scale;
            assert!(
                on_screen.x <= FREE.x && on_screen.y <= FREE.y,
                "{grid:?} comes out {on_screen:?} in {FREE:?}"
            );
        }
    }

    /// And it fits with room to spare on every side: the compass words are
    /// written into that gap, outside the board. Too little air and "north"
    /// would be printed over the top row of cells.
    #[test]
    fn every_board_keeps_air_for_the_compass() {
        for grid in [
            Grid::default(),
            Grid {
                width: 24,
                height: 16,
            },
            Grid {
                width: 200,
                height: 4,
            },
            Grid {
                width: 4,
                height: 200,
            },
        ] {
            let on_screen = board_size(grid) / View::fit(grid, FREE);
            let air = (FREE - on_screen) * 0.5;
            assert!(
                air.x >= BOARD_MARGIN - 0.001 && air.y >= BOARD_MARGIN - 0.001,
                "{grid:?} leaves only {air:?} around it"
            );
        }
    }

    /// A small board is magnified rather than left as a stamp in the middle,
    /// and a large one is pulled back until it fits.
    #[test]
    fn small_boards_are_zoomed_in_and_large_ones_out() {
        let small = View::fit(
            Grid {
                width: 24,
                height: 16,
            },
            FREE,
        );
        assert!(small < 1.0, "level 1 should fill the window, got {small}");

        let large = View::fit(
            Grid {
                width: 120,
                height: 80,
            },
            FREE,
        );
        assert!(large > 1.0, "a board that big has to be pulled back");
    }

    /// The mapping the labels use forwards and the click picker uses backwards
    /// has to be the same mapping, or a click lands next to the ant it looks
    /// like it hit.
    #[test]
    fn screen_and_world_are_inverses() {
        let view = View {
            scale: 0.53,
            origin: Vec2::new(638.0, 397.0),
            window_centre: Vec2::new(650.0, 390.0),
        };
        for world in [
            Vec2::ZERO,
            Vec2::new(120.0, -340.0),
            Vec2::new(-640.0, 320.0),
        ] {
            let there_and_back = view.to_world(view.to_screen(world));
            assert!(
                there_and_back.distance(world) < 0.001,
                "{world:?} came back as {there_and_back:?}"
            );
        }
    }
}
