pub mod fruit;
pub mod grid;
pub mod nest;
pub mod render;
pub mod scent;
pub mod vision;

use bevy::prelude::*;

use fruit::Regrowth;
use grid::{Grid, Occupancy};
use nest::{Nest, Stores};
use scent::Scent;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        let grid = Grid::default();
        app.insert_resource(grid)
            .insert_resource(Occupancy::new(grid))
            .insert_resource(Scent::new(grid))
            .insert_resource(render::clear_color())
            .init_resource::<Nest>()
            .init_resource::<Stores>()
            .init_resource::<Regrowth>()
            .add_systems(
                Startup,
                (
                    render::spawn_camera,
                    render::spawn_board,
                    render::spawn_nest_frame,
                    fruit::plant_first_fruits,
                ),
            )
            .add_systems(
                Update,
                (fruit::regrow, scent::evaporate, render::show_scent),
            );
    }
}
