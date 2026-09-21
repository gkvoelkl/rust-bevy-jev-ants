pub mod grid;
pub mod render;
pub mod vision;

use bevy::prelude::*;

use grid::{Grid, Occupancy};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        let grid = Grid::default();
        app.insert_resource(grid)
            .insert_resource(Occupancy::new(grid))
            .insert_resource(render::clear_color())
            .add_systems(Startup, (render::spawn_camera, render::spawn_board));
    }
}
