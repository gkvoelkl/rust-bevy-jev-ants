pub mod components;
pub mod intent;
pub mod movement;

use bevy::prelude::*;
use rand::seq::{IndexedRandom, SliceRandom};

use crate::config::{ANT_COUNT, CELL_SIZE};
use crate::decisions::{ThinkSet, ThinkTimer};
use crate::world::grid::{Dir, Grid, GridPos, Occupancy, Occupant};
use crate::world::nest::Nest;
use crate::world::render::Z_ANT;

use components::{AntId, Carrying, Facing, LastDecision, SinceNest};

pub const BODY: Color = Color::srgb(0.85, 0.42, 0.18);
const HEAD: Color = Color::srgb(0.96, 0.72, 0.36);

pub struct AntsPlugin;

impl Plugin for AntsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ants).add_systems(
            Update,
            (
                movement::apply_decisions,
                intent::pursue_intents,
                movement::animate_steps,
                movement::face_direction,
            )
                .chain()
                .after(ThinkSet),
        );
    }
}

fn spawn_ants(
    mut commands: Commands,
    grid: Res<Grid>,
    nest: Res<Nest>,
    mut occupancy: ResMut<Occupancy>,
) {
    let grid = *grid;
    let mut rng = rand::rng();

    // The colony starts at home and has to leave the nest before it can do
    // anything. Should there ever be more ants than nest cells, the rest start
    // outside rather than going missing.
    let mut cells: Vec<IVec2> = nest.cells().collect();
    cells.shuffle(&mut rng);

    let mut outside: Vec<IVec2> = grid.cells().filter(|cell| !nest.contains(*cell)).collect();
    outside.shuffle(&mut rng);
    cells.extend(outside);

    // Each cell only once, and only if nothing stands there already.
    cells.retain(|cell| occupancy.is_free(grid, *cell));

    for (index, cell) in cells.into_iter().take(ANT_COUNT).enumerate() {
        let facing = *Dir::COMPASS.choose(&mut rng).expect("COMPASS is not empty");
        let ant = commands
            .spawn((
                Name::new(format!("ant_{index:02}")),
                AntId(index as u32),
                GridPos(cell),
                ThinkTimer::staggered(index, ANT_COUNT),
                Facing(facing),
                LastDecision::default(),
                Carrying::default(),
                SinceNest::default(),
                Sprite::from_color(BODY, Vec2::new(CELL_SIZE * 0.34, CELL_SIZE * 0.56)),
                Transform::from_translation(grid.to_screen(cell).extend(Z_ANT))
                    .with_rotation(Quat::from_rotation_z(facing.angle())),
                children![(
                    Sprite::from_color(HEAD, Vec2::splat(CELL_SIZE * 0.2)),
                    Transform::from_xyz(0.0, CELL_SIZE * 0.3, 0.1),
                )],
            ))
            .id();
        occupancy.occupy(grid, cell, Occupant::Ant(ant));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FRUIT_TARGET;
    use crate::decisions::{DecisionsFrom, DecisionsPlugin};
    use crate::world::WorldPlugin;
    use crate::world::fruit::Fruit;
    use crate::world::grid::GridPos;
    use crate::world::nest::{Nest, Stores};
    use crate::world::scent::Scent;
    use bevy::platform::collections::HashSet;
    use components::MoveAnim;
    use std::time::Duration;

    /// Drives the whole simulation headless. `Time` is advanced by hand, so a
    /// simulated minute costs milliseconds and the result does not depend on how
    /// fast the machine runs.
    fn run_headless(frames: usize, step: Duration) {
        let mut app = App::new();
        // No TimePlugin on purpose — the test owns the clock.
        app.insert_resource(Time::<()>::default());
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
        ));
        app.finish();
        app.cleanup();

        for frame in 0..frames {
            app.world_mut().resource_mut::<Time>().advance_by(step);
            app.update();

            // Fruits go into the set first, so an ant standing on one trips the
            // same assertion as two ants sharing a cell.
            let nest = *app.world().resource::<Nest>();
            let mut taken: HashSet<IVec2> = HashSet::new();

            let mut fruits = app.world_mut().query_filtered::<&GridPos, With<Fruit>>();
            for position in fruits.iter(app.world()) {
                assert!(
                    !nest.contains(position.0),
                    "a fruit grew inside the nest at {:?}",
                    position.0
                );
                assert!(
                    taken.insert(position.0),
                    "two fruits on cell {:?} in frame {frame}",
                    position.0
                );
            }

            let mut query = app
                .world_mut()
                .query_filtered::<(&GridPos, Option<&MoveAnim>), With<AntId>>();
            for (position, moving) in query.iter(app.world()) {
                assert!(
                    taken.insert(position.0),
                    "cell {:?} is taken twice in frame {frame}",
                    position.0
                );
                if let Some(anim) = moving {
                    assert!(
                        taken.insert(anim.to),
                        "an ant walks into the taken cell {:?} in frame {frame}",
                        anim.to
                    );
                }
            }
        }
    }

    #[test]
    fn ants_never_share_a_cell() {
        // 3600 frames at 50 ms is a simulated minute, roughly 60 decision rounds.
        run_headless(3600, Duration::from_millis(50));
    }

    /// The acceptance test for the simulation: driven by the rule baseline —
    /// never by the model — fruit gets fetched, carried and delivered. It
    /// covers the classical half of the game: intents, pathfinding, pickup,
    /// drop and the scent trail that makes the way home findable.
    #[test]
    fn the_simulation_delivers_fruit_when_driven_by_rules() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
        ));
        app.finish();
        app.cleanup();
        app.update();

        let mut fruits = app.world_mut().query::<&Fruit>();
        assert_eq!(
            fruits.iter(app.world()).count(),
            FRUIT_TARGET,
            "the board starts stocked"
        );

        // Half a simulated minute.
        for _ in 0..600 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        let stored = app.world().resource::<Stores>().0;
        assert!(
            stored >= 5,
            "the classic rules delivered only {stored} fruits in 30 seconds"
        );

        let mut fruits = app.world_mut().query::<&Fruit>();
        assert!(
            fruits.iter(app.world()).count() <= FRUIT_TARGET,
            "regrowth tops up, it does not stack up"
        );
    }

    /// The colony wakes up at home, and the first thing it has to do is get out.
    #[test]
    fn the_colony_starts_in_the_nest() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
        ));
        app.finish();
        app.cleanup();
        app.update();

        let nest = *app.world().resource::<Nest>();
        let mut ants = app.world_mut().query_filtered::<&GridPos, With<AntId>>();

        let positions: Vec<IVec2> = ants.iter(app.world()).map(|position| position.0).collect();
        assert_eq!(positions.len(), ANT_COUNT);
        for position in &positions {
            assert!(nest.contains(*position), "{position:?} is not the nest");
        }
        assert_eq!(
            positions.iter().collect::<HashSet<_>>().len(),
            ANT_COUNT,
            "no two ants on one cell"
        );
    }

    /// The trails are laid in the real loop, not only in the unit test: after
    /// half a minute of carrying, the ground remembers where the fruit was.
    #[test]
    fn carrying_ants_leave_trails_behind() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
        ));
        app.finish();
        app.cleanup();

        for _ in 0..600 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        let grid = *app.world().resource::<Grid>();
        let scent = app.world().resource::<Scent>();
        let scented = grid
            .cells()
            .filter(|cell| scent.at(grid, *cell) > 0.0)
            .count();

        assert!(
            scented >= 10,
            "only {scented} cells carry scent — nobody marked a way"
        );
    }

    #[test]
    fn every_ant_stays_on_the_board() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
        ));
        app.finish();
        app.cleanup();

        for _ in 0..1200 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        let grid = *app.world().resource::<Grid>();
        let mut query = app.world_mut().query_filtered::<&GridPos, With<AntId>>();
        let positions: Vec<IVec2> = query.iter(app.world()).map(|p| p.0).collect();
        assert_eq!(positions.len(), ANT_COUNT);
        for position in positions {
            assert!(grid.contains(position), "{position:?} is off the board");
        }
    }
}
