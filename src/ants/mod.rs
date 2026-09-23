pub mod components;
pub mod intent;
pub mod movement;

use bevy::prelude::*;
use rand::seq::{IndexedRandom, SliceRandom};

use crate::config::{ANT_COUNT, CELL_SIZE};
use crate::decisions::{RecordSet, ThinkSet, ThinkTimer};
use crate::world::grid::{Dir, Grid, GridPos, Occupancy, Occupant};
use crate::world::nest::Nest;
use crate::world::render::Z_ANT;

use components::{AntId, Carrying, Facing, LastDecision, LastExchange, SinceNest};

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
                intent::drop_abandoned_planks,
                movement::animate_steps,
                movement::face_direction,
            )
                .chain()
                .after(ThinkSet)
                // `apply_decisions` is what drains the answers; the records can
                // only be picked up once it has.
                .before(RecordSet),
        );
    }
}

/// Public so `level.rs` can run it again when another level is picked.
pub fn spawn_ants(
    mut commands: Commands,
    grid: Res<Grid>,
    nest: Res<Nest>,
    mut occupancy: ResMut<Occupancy>,
    scenario: Option<Res<crate::world::scenario::Scenario>>,
) {
    let grid = *grid;
    let mut rng = rand::rng();
    // A task board sets its own colony size: "two of you" means something with
    // eight ants and very little with twenty.
    let count = scenario.map_or(ANT_COUNT, |scenario| scenario.ants);

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

    for (index, cell) in cells.into_iter().take(count).enumerate() {
        let facing = *Dir::COMPASS.choose(&mut rng).expect("COMPASS is not empty");
        let ant = commands
            .spawn((
                Name::new(format!("ant_{index:02}")),
                crate::world::LevelEntity,
                AntId(index as u32),
                GridPos(cell),
                ThinkTimer::staggered(index, count),
                Facing(facing),
                LastDecision::default(),
                LastExchange::default(),
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
    use crate::decisions::{
        ColonyAwake, DecisionStats, DecisionsFrom, DecisionsPlugin, QueenOrder,
    };
    use crate::world::WorldPlugin;
    use crate::world::fruit::Fruit;
    use crate::world::grid::GridPos;
    use crate::world::nest::{Nest, Stores};
    use crate::world::scent::Scent;
    use bevy::platform::collections::HashSet;
    use components::MoveAnim;
    use intent::Intent;
    use std::time::Duration;

    /// Any order at all. What it says does not matter to the rule baseline —
    /// only that it was said, because that is what wakes the colony.
    const AN_ORDER: &str = "fetch fruit and bring it home";

    /// A colony on the rule baseline, with the clock in the test's hands: no
    /// `TimePlugin`, so a simulated minute costs milliseconds and the result
    /// does not depend on how fast the machine runs.
    ///
    /// The order is handed over at build time because an unspoken queen means a
    /// colony that never asks anything and never moves.
    fn colony(order: &str) -> App {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
        ));
        app.insert_resource(QueenOrder(order.to_string()));
        app.finish();
        app.cleanup();
        app
    }

    /// Drives the whole simulation headless.
    fn run_headless(frames: usize, step: Duration) {
        let mut app = colony(AN_ORDER);

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
        let mut app = colony(AN_ORDER);
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
        let mut app = colony(AN_ORDER);
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
        let mut app = colony(AN_ORDER);

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

    /// The opening: twenty ants at home, and not one request in the air. An ant
    /// knows what it can see, not what it is for — so until the queen says
    /// something, nothing is asked and nothing is paid for.
    #[test]
    fn the_colony_sleeps_until_the_queen_speaks() {
        let mut app = colony("");

        // Ten simulated seconds, five think intervals.
        for _ in 0..200 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        assert!(!app.world().resource::<ColonyAwake>().0);

        let stats = app.world().resource::<DecisionStats>();
        assert_eq!(
            (stats.from_rules, stats.from_jev, stats.discarded),
            (0, 0, 0),
            "a sleeping colony must not ask anything at all"
        );

        let nest = *app.world().resource::<Nest>();
        let mut ants = app
            .world_mut()
            .query_filtered::<(&GridPos, Option<&Intent>), With<AntId>>();
        for (position, intent) in ants.iter(app.world()) {
            assert!(nest.contains(position.0), "{:?} left the nest", position.0);
            assert!(intent.is_none(), "an ant is pursuing something already");
        }
    }

    /// And the first order is what starts everything: every ant is asked in that
    /// one frame, rather than waiting out its own interval first.
    #[test]
    fn the_first_order_sets_the_whole_colony_off() {
        let mut app = colony("");
        app.update();

        app.insert_resource(QueenOrder("go and look for fruit".to_string()));
        app.update();

        assert!(app.world().resource::<ColonyAwake>().0);
        assert_eq!(
            app.world().resource::<DecisionStats>().from_rules as usize,
            ANT_COUNT,
            "the first order reaches every ant at once, not one by one"
        );

        // Eight simulated seconds: two or three wanders, enough to be out.
        for _ in 0..160 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        let nest = *app.world().resource::<Nest>();
        let mut ants = app.world_mut().query_filtered::<&GridPos, With<AntId>>();
        let outside = ants
            .iter(app.world())
            .filter(|position| !nest.contains(position.0))
            .count();
        assert!(
            outside >= ANT_COUNT / 2,
            "only {outside} of {ANT_COUNT} ants left the nest after the order"
        );
    }

    /// The guard against the storm this game has already had once: a resource
    /// that is only *read* must not count as changed, or every ant is asked
    /// again on every frame. Sixty frames a second against twenty ants is
    /// twelve thousand requests in ten seconds, and a real bill to go with it.
    #[test]
    fn nobody_is_asked_more_than_their_interval_allows() {
        let mut app = colony(AN_ORDER);

        // Ten simulated seconds at sixty frames a second.
        for _ in 0..600 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(16));
            app.update();
        }

        let asked = app.world().resource::<DecisionStats>().from_rules;
        // Five intervals for twenty ants is a hundred; the rest is ants that
        // finished an intent early and asked again, which is meant to happen.
        assert!(
            asked < 1000,
            "{asked} decisions in ten seconds — something is asking every frame"
        );
    }

    /// The trail has to actually reach the ants, not just exist.
    ///
    /// Measured rather than assumed, because this is the number a bigger board
    /// quietly breaks: a deposit is `0.88^steps` and falls under the threshold
    /// after about 23 steps from the nest, so widening the board shrinks the
    /// share of the colony that can smell anything. At 64 x 32 it sits near
    /// two thirds; if it ever drops to nothing, the pheromones have become
    /// decoration.
    #[test]
    fn most_ants_can_smell_a_trail() {
        use crate::config::VISION_RADIUS;
        use crate::world::vision::sightings;

        let mut app = colony(AN_ORDER);
        // A simulated minute, long enough for the colony to have laid one.
        for _ in 0..1200 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        let grid = *app.world().resource::<Grid>();
        let nest = *app.world().resource::<Nest>();

        let mut query = app.world_mut().query_filtered::<&GridPos, With<AntId>>();
        let where_they_stand: Vec<IVec2> = query.iter(app.world()).map(|p| p.0).collect();

        let scent = app.world().resource::<Scent>();
        let occupancy = app.world().resource::<Occupancy>();
        let smelling = where_they_stand
            .iter()
            .filter(|at| {
                sightings(grid, occupancy, nest, scent, **at, VISION_RADIUS)
                    .iter()
                    .any(|line| line.contains("scent trail"))
            })
            .count();

        assert!(
            smelling >= ANT_COUNT / 3,
            "only {smelling} of {ANT_COUNT} ants have a trail within sight — \
             the board may have outgrown the trail's reach"
        );
    }

    /// Everybody follows the plain rules, except ant 0, which behaves as if the
    /// queen had said *"head south-east, and if you see the plank, take it"*.
    ///
    /// Both halves of that sentence are needed, and the second alone is not
    /// enough: the plank is visible from eight cells, the nest is nine cells
    /// from it at the far corner, so an ant left to wander may take a minute to
    /// stumble on it or never do. Steering it is not the test cheating — it is
    /// the test standing in for the one thing this game has instead of
    /// waypoints, which is a sentence.
    ///
    /// The other seven follow the plain rules, and that matters too. Left as
    /// statues they filled the only way through and the carrier wedged itself
    /// against them — which is how the missing end to `HoldPlank` came to
    /// light.
    #[derive(Clone, Default)]
    struct PlankMinded {
        ready: std::sync::Arc<std::sync::Mutex<Vec<crate::decisions::AntMove>>>,
        let_go: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl crate::decisions::DecisionSource for PlankMinded {
        fn request(&mut self, ant: &crate::decisions::AntView<'_>) {
            use crate::decisions::Action;
            let wanted = if ant.id.0 != 0 {
                None
            } else if self.let_go.load(std::sync::atomic::Ordering::Relaxed) {
                ant.options
                    .iter()
                    .copied()
                    .find(|option| matches!(option, Action::LetGoPlank))
            } else {
                ant.options
                    .iter()
                    .copied()
                    .find(|option| matches!(option, Action::TakePlank { .. }))
                    .or_else(|| {
                        ant.options
                            .iter()
                            .copied()
                            .find(|option| matches!(option, Action::Walk(Dir::SouthEast)))
                    })
            };

            self.ready.lock().expect("no other thread holds this").push(
                crate::decisions::AntMove {
                    id: ant.id,
                    action: wanted.unwrap_or_else(|| crate::decisions::rules::decide(ant)),
                    origin: crate::decisions::Origin::Rules,
                },
            );
        }

        fn poll(&mut self) -> Vec<crate::decisions::AntMove> {
            std::mem::take(&mut *self.ready.lock().expect("no other thread holds this"))
        }

        fn name(&self) -> &'static str {
            "plank-minded rules"
        }
    }

    /// The task board, played through the real decision path.
    fn task_board() -> (App, PlankMinded) {
        use crate::decisions::{ActiveSource, QueenOrder};
        use crate::world::scenario::Scenario;

        let source = PlankMinded::default();
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.insert_resource(Scenario::load("plank").expect("the board reads"));
        app.add_plugins((
            WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::RulesForTesting,
            },
            AntsPlugin,
        ));
        // Replaces the plain rule source the plugin just put in.
        app.insert_resource(ActiveSource(Box::new(source.clone())));
        // The colony sleeps until it is spoken to, and a sleeping colony asks
        // nothing at all.
        app.insert_resource(QueenOrder("go and work".to_string()));
        app.finish();
        app.cleanup();
        app.update();
        (app, source)
    }

    fn the_plank(app: &mut App) -> Entity {
        let mut query = app
            .world_mut()
            .query_filtered::<Entity, With<crate::world::plank::Plank>>();
        query
            .iter(app.world())
            .next()
            .expect("the board has a plank")
    }

    fn bridges(app: &mut App) -> usize {
        let grid = *app.world().resource::<Grid>();
        let occupancy = app.world().resource::<Occupancy>();
        grid.cells()
            .filter(|cell| occupancy.terrain_at(grid, *cell) == crate::world::grid::Terrain::Bridge)
            .count()
    }

    /// Runs until `done` is true, up to `limit` simulated seconds. Waiting for
    /// the state rather than counting out seconds: how long the colony takes
    /// depends on how it happens to be standing, and a fixed number would be
    /// either flaky or so generous it tested nothing.
    fn run_until(app: &mut App, limit: f32, done: impl Fn(&App) -> bool) -> bool {
        for _ in 0..((limit / 0.05) as u32) {
            if done(app) {
                return true;
            }
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }
        done(app)
    }

    fn carrier(app: &App, plank: Entity) -> Option<Entity> {
        app.world()
            .get::<crate::world::plank::Plank>(plank)
            .and_then(|state| state.carrier)
    }

    /// The task, end to end: an ant is offered the plank, takes it, walks it to
    /// the river on its own and lays it across.
    #[test]
    fn an_ant_bridges_the_river() {
        let (mut app, _source) = task_board();
        assert_eq!(bridges(&mut app), 0, "the river starts uncrossed");

        let crossed = run_until(&mut app, 120.0, |app| {
            let grid = *app.world().resource::<Grid>();
            let occupancy = app.world().resource::<Occupancy>();
            grid.cells()
                .any(|cell| occupancy.terrain_at(grid, cell) == crate::world::grid::Terrain::Bridge)
        });

        assert!(crossed, "the plank never reached the water");
        assert_eq!(bridges(&mut app), 1, "exactly one crossing, not a row");
    }

    /// Told to do something else halfway, the ant puts the plank down rather
    /// than dragging it around invisibly for the rest of the game. The board
    /// owns the thing, not the decision that picked it up.
    #[test]
    fn a_plank_let_go_of_lands_back_on_the_ground() {
        use crate::decisions::QueenOrder;
        use std::sync::atomic::Ordering;

        let (mut app, source) = task_board();
        let plank = the_plank(&mut app);

        let picked_up = run_until(&mut app, 60.0, |app| carrier(app, plank).is_some());
        assert!(picked_up, "the ant never picked the plank up");

        // The queen speaks, which is the one thing that interrupts an intent —
        // and this time the answer is to put it down.
        source.let_go.store(true, Ordering::Relaxed);
        app.world_mut().resource_mut::<QueenOrder>().0 = "leave it".to_string();

        let dropped = run_until(&mut app, 20.0, |app| carrier(app, plank).is_none());
        assert!(dropped, "the ant is still holding it");
        assert!(
            app.world().get::<GridPos>(plank).is_some(),
            "and it is back on a cell, where another ant can find it"
        );
        assert_eq!(bridges(&mut app), 0, "it never reached the water");
    }

    #[test]
    fn every_ant_stays_on_the_board() {
        let mut app = colony(AN_ORDER);

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
