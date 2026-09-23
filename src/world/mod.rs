pub mod fruit;
pub mod grid;
pub mod nest;
pub mod plank;
pub mod render;
pub mod scenario;
pub mod scent;
pub mod vision;

use bevy::prelude::*;

use fruit::Regrowth;
use grid::{Grid, Occupancy, Terrain};
use nest::{Nest, Stores};
use scenario::Scenario;
use scent::Scent;

/// Everything that belongs to the level being played and goes away when
/// another one is picked. The camera deliberately has none.
#[derive(Component)]
pub struct LevelEntity;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        // A scenario, if one was inserted before this plugin, decides the shape
        // of everything. Without one the open field is unchanged, which is what
        // keeps every existing test honest.
        let scenario = app.world().get_resource::<Scenario>().cloned();

        let grid = scenario.as_ref().map_or_else(Grid::default, Scenario::grid);
        let nest = scenario.as_ref().map_or_else(Nest::default, Scenario::nest);

        let mut occupancy = Occupancy::new(grid);
        if let Some(scenario) = &scenario {
            // The river is dug before anything stands on the board, so nothing
            // can ever be placed into it.
            for (x, y) in &scenario.water {
                occupancy.set_terrain(grid, IVec2::new(*x, *y), Terrain::Water);
            }
        }

        app.insert_resource(grid)
            .insert_resource(nest)
            .insert_resource(occupancy)
            .insert_resource(Scent::new(grid))
            .insert_resource(render::clear_color())
            .init_resource::<render::View>()
            .init_resource::<Stores>()
            .init_resource::<Regrowth>()
            .init_resource::<scenario::Solved>()
            .add_systems(
                Startup,
                (
                    render::spawn_camera,
                    render::spawn_board,
                    render::spawn_nest_frame,
                    fruit::plant_first_fruits,
                    plank::spawn_plank,
                ),
            )
            .add_systems(
                Update,
                (
                    fruit::regrow,
                    scent::evaporate,
                    render::show_scent,
                    render::fit_camera,
                    plank::carried_plank_follows,
                    scenario::check_solved,
                ),
            );
    }
}
