//! Starting a level, and starting it again.
//!
//! This is the one module allowed to know both the world and the ants, because
//! that is what a level is: a board, a colony on it, and a task. Everything
//! else keeps the direction of dependencies `ANTS.md` §6 asks for.
//!
//! Restarting matters more than it sounds. The loop this game is about — say a
//! sentence, watch it fail, say a better one — needs the second half. Without a
//! way back to the start there is no second attempt, and without a second
//! attempt there is nothing to learn from the first.

use bevy::prelude::*;

use crate::ants::spawn_ants;
use crate::decisions::{ColonyAwake, DecisionStats, DiscardLog, QueenOrder};
use crate::ui::inspector::Selected;
use crate::world::LevelEntity;
use crate::world::fruit::plant_first_fruits;
use crate::world::grid::{Grid, Occupancy, Terrain};
use crate::world::nest::{Nest, Stores};
use crate::world::plank::spawn_plank;
use crate::world::render::{spawn_board, spawn_nest_frame};
use crate::world::scenario::{Scenario, Solved};
use crate::world::scent::Scent;

/// What the player picked. The open field is a level like any other here — it
/// simply has no task.
#[derive(Clone)]
pub enum Choice {
    OpenField,
    Task(Box<Scenario>),
}

/// A level chosen but not yet started. Set by the picker, taken by
/// `start_chosen_level` on the next frame.
#[derive(Resource, Default)]
pub struct Chosen(pub Option<Choice>);

/// The levels on offer, read once at startup so the picker never touches the
/// file system while the game runs.
#[derive(Resource, Default)]
pub struct Levels(pub Vec<Scenario>);

pub struct LevelPlugin;

impl Plugin for LevelPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Levels(Scenario::all()))
            .init_resource::<Chosen>()
            // In `Last`, not `Update`, and that placement is load-bearing.
            // Tearing the level down means despawning every ant — and systems
            // in `Update` have queued commands against those same ants this
            // frame. A despawn that lands before them turns the next
            // `insert(MoveAnim)` into a panic on a dead entity. `Last` runs
            // once `Update` has applied its buffers, so there is nothing left
            // in flight to trip over.
            .add_systems(Last, start_chosen_level);
    }
}

/// What a fresh level resets. Grouped because the list is long and every entry
/// is the same kind of thing: state that belonged to the attempt just ended.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Board<'w> {
    grid: ResMut<'w, Grid>,
    nest: ResMut<'w, Nest>,
    occupancy: ResMut<'w, Occupancy>,
    scent: ResMut<'w, Scent>,
    stores: ResMut<'w, Stores>,
    solved: ResMut<'w, Solved>,
}

/// What the colony carries over from one attempt to the next: nothing.
#[derive(bevy::ecs::system::SystemParam)]
pub struct Colony<'w> {
    awake: ResMut<'w, ColonyAwake>,
    order: ResMut<'w, QueenOrder>,
    stats: ResMut<'w, DecisionStats>,
    discards: ResMut<'w, DiscardLog>,
    selected: ResMut<'w, Selected>,
}

fn start_chosen_level(
    mut commands: Commands,
    mut chosen: ResMut<Chosen>,
    mut board: Board,
    mut colony: Colony,
    old: Query<Entity, With<LevelEntity>>,
) {
    let Some(choice) = chosen.0.take() else {
        return;
    };
    let scenario = match &choice {
        Choice::OpenField => None,
        Choice::Task(scenario) => Some(scenario.as_ref().clone()),
    };

    // Everything that belonged to the last attempt. The camera has no
    // `LevelEntity`, so it stays where it is.
    for entity in &old {
        commands.entity(entity).despawn();
    }

    let grid = scenario.as_ref().map_or_else(Grid::default, Scenario::grid);
    let nest = scenario.as_ref().map_or_else(Nest::default, Scenario::nest);

    let mut fresh = Occupancy::new(grid);
    if let Some(scenario) = &scenario {
        for (x, y) in &scenario.water {
            fresh.set_terrain(grid, IVec2::new(*x, *y), Terrain::Water);
        }
    }

    *board.grid = grid;
    *board.nest = nest;
    *board.occupancy = fresh;
    *board.scent = Scent::new(grid);
    board.stores.0 = 0;
    board.solved.0 = false;

    // The colony starts as it always does: at home, asleep, waiting to be
    // spoken to. Carrying the last attempt's order over would rob the new
    // board of its opening.
    colony.awake.0 = false;
    colony.order.0.clear();
    // Not a fresh `DecisionStats`: what has been spent stays spent, and only
    // the per-level request count starts again. The discard log is a rolling
    // window of the last few answers, so it does start empty — an ant number
    // from the board before this one would mean nothing.
    colony.stats.start_new_level();
    *colony.discards = DiscardLog::default();
    colony.selected.0 = None;

    match scenario {
        Some(scenario) => {
            info!("starting level '{}'", scenario.name);
            commands.insert_resource(scenario);
        }
        None => {
            info!("back to the open field");
            commands.remove_resource::<Scenario>();
        }
    }

    // Queued behind the despawns and the resource change, and run in this
    // order: ground first, then what stands on it.
    commands.run_system_cached(spawn_board);
    commands.run_system_cached(spawn_nest_frame);
    commands.run_system_cached(plant_first_fruits);
    commands.run_system_cached(spawn_plank);
    commands.run_system_cached(spawn_ants);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ants::AntsPlugin;
    use crate::ants::components::AntId;
    use crate::decisions::{DecisionsFrom, DecisionsPlugin};
    use crate::world::WorldPlugin;
    use crate::world::fruit::Fruit;
    use crate::world::grid::GridPos;
    use crate::world::plank::Plank;
    use std::time::Duration;

    fn game() -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
            LevelPlugin,
        ));
        app.init_resource::<Selected>();
        app.finish();
        app.cleanup();
        app.update();
        app
    }

    fn pick(app: &mut App, level: Option<usize>) {
        let choice = match level {
            None => Choice::OpenField,
            Some(index) => {
                let scenario = app.world().resource::<Levels>().0[index].clone();
                Choice::Task(Box::new(scenario))
            }
        };
        app.world_mut().resource_mut::<Chosen>().0 = Some(choice);
        // Two updates: one to tear down and queue, one to let the queued spawns
        // run.
        app.update();
        app.update();
    }

    fn count<T: Component>(app: &mut App) -> usize {
        let mut query = app.world_mut().query_filtered::<Entity, With<T>>();
        query.iter(app.world()).count()
    }

    /// Picking a level starts it: the old board is gone, the new one is there,
    /// and the colony is back in its nest.
    #[test]
    fn choosing_a_level_starts_it() {
        let mut app = game();

        pick(&mut app, Some(1)); // the plank board
        let scenario = app.world().resource::<Scenario>().clone();
        assert_eq!(count::<Fruit>(&mut app), scenario.fruit.len());
        assert_eq!(count::<AntId>(&mut app), scenario.ants);
        assert_eq!(count::<Plank>(&mut app), 1, "this board has a plank");

        let nest = *app.world().resource::<Nest>();
        let mut ants = app.world_mut().query_filtered::<&GridPos, With<AntId>>();
        for at in ants.iter(app.world()) {
            assert!(nest.contains(at.0), "the colony starts at home");
        }
    }

    /// And picking another one leaves nothing of the first behind — no fruit
    /// from the old board, no plank where this level has none, no ants from a
    /// colony that was a different size.
    #[test]
    fn switching_levels_leaves_nothing_behind() {
        let mut app = game();

        pick(&mut app, Some(1));
        assert_eq!(count::<Plank>(&mut app), 1);

        pick(&mut app, Some(0)); // the fruit board, which has no plank
        let scenario = app.world().resource::<Scenario>().clone();
        assert_eq!(count::<Plank>(&mut app), 0, "the plank came along");
        assert_eq!(count::<Fruit>(&mut app), scenario.fruit.len());
        assert_eq!(count::<AntId>(&mut app), scenario.ants);

        let grid = *app.world().resource::<Grid>();
        assert_eq!((grid.width, grid.height), (scenario.width, scenario.height));

        pick(&mut app, None); // and back to the open field
        assert!(app.world().get_resource::<Scenario>().is_none());
        assert_eq!(count::<AntId>(&mut app), crate::config::ANT_COUNT);
    }

    /// A fresh level is a fresh attempt: stores at nothing, the task unsolved,
    /// and the colony asleep again until the queen speaks.
    #[test]
    fn a_new_level_starts_from_nothing() {
        let mut app = game();
        pick(&mut app, Some(0));

        app.world_mut().resource_mut::<Stores>().0 = 4;
        app.world_mut().resource_mut::<ColonyAwake>().0 = true;
        app.world_mut().resource_mut::<QueenOrder>().0 = "go east".to_string();

        pick(&mut app, Some(1));

        assert_eq!(app.world().resource::<Stores>().0, 0);
        assert!(!app.world().resource::<Solved>().0);
        assert!(!app.world().resource::<ColonyAwake>().0, "asleep again");
        assert!(app.world().resource::<QueenOrder>().0.is_empty());
    }

    /// The wallet survives the level picker. Tokens already sent are already
    /// paid for, and a counter that forgot them every time the player tried
    /// another board would understate the bill by however many boards they
    /// tried.
    #[test]
    fn the_bill_carries_across_levels() {
        let mut app = game();
        pick(&mut app, Some(0));

        {
            let mut stats = app.world_mut().resource_mut::<DecisionStats>();
            stats.record(&crate::decisions::Origin::Jev {
                confidence: 0.9,
                latency_ms: 700,
                input_tokens: 180,
            });
            assert_eq!(stats.this_level, 1);
        }

        pick(&mut app, Some(1));

        let stats = app.world().resource::<DecisionStats>();
        assert_eq!(
            stats.input_tokens, 180,
            "the tokens were spent all the same"
        );
        assert_eq!(stats.from_jev, 1, "and so was the request");
        assert!(stats.cost_usd() > 0.0, "the money did not come back");
        assert_eq!(
            stats.this_level, 0,
            "but the new attempt counts its own requests"
        );
    }

    /// The board really is rebuilt, not just re-measured: the river of the
    /// plank level has to be in the ground when its ants are put down on it.
    #[test]
    fn the_river_is_there_before_the_colony_is() {
        let mut app = game();
        pick(&mut app, Some(1));

        let grid = *app.world().resource::<Grid>();
        let scenario = app.world().resource::<Scenario>().clone();
        {
            let occupancy = app.world().resource::<Occupancy>();
            for (x, y) in &scenario.water {
                assert_eq!(
                    occupancy.terrain_at(grid, IVec2::new(*x, *y)),
                    Terrain::Water,
                    "({x}, {y}) should be river"
                );
            }
        }

        // And it drains again on the way back to the open field.
        pick(&mut app, None);
        let grid = *app.world().resource::<Grid>();
        let occupancy = app.world().resource::<Occupancy>();
        assert!(
            grid.cells()
                .all(|cell| occupancy.terrain_at(grid, cell) == Terrain::Ground),
            "the open field has no water"
        );
    }

    /// The open field is restarted like any other board, which is what the
    /// bar's Restart button does when no scenario is in force: same choice
    /// again, colony home, fruit back.
    #[test]
    fn the_open_field_restarts_too() {
        let mut app = game();
        pick(&mut app, None);

        app.world_mut().resource_mut::<ColonyAwake>().0 = true;
        let fruit = count::<Fruit>(&mut app);
        for _ in 0..200 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        pick(&mut app, None);

        assert!(app.world().get_resource::<Scenario>().is_none());
        assert!(!app.world().resource::<ColonyAwake>().0, "asleep again");
        assert_eq!(
            count::<Fruit>(&mut app),
            fruit,
            "the field is stocked again"
        );
        let nest = *app.world().resource::<Nest>();
        let mut ants = app.world_mut().query_filtered::<&GridPos, With<AntId>>();
        assert_eq!(ants.iter(app.world()).count(), crate::config::ANT_COUNT);
        for at in ants.iter(app.world()) {
            assert!(nest.contains(at.0), "everybody is home again");
        }
    }

    /// Restarting the level you are on is the loop this game is for: try a
    /// sentence, watch it fail, try another on the same board.
    #[test]
    fn a_level_can_be_restarted_from_the_middle_of_itself() {
        let mut app = game();
        pick(&mut app, Some(0));

        app.world_mut().resource_mut::<ColonyAwake>().0 = true;
        app.world_mut().resource_mut::<QueenOrder>().0 = "spread out".to_string();
        for _ in 0..200 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        pick(&mut app, Some(0));

        let scenario = app.world().resource::<Scenario>().clone();
        assert_eq!(count::<Fruit>(&mut app), scenario.fruit.len(), "all back");
        let nest = *app.world().resource::<Nest>();
        let mut ants = app.world_mut().query_filtered::<&GridPos, With<AntId>>();
        assert_eq!(ants.iter(app.world()).count(), scenario.ants);
        for at in ants.iter(app.world()) {
            assert!(nest.contains(at.0), "everybody is home again");
        }
    }
}
