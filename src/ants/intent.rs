//! Carrying an intent out, over several ticks and several cells.
//!
//! This is the classical half of the two layers: pathfinding, picking up,
//! putting down. No model is involved here. The decision layer only ever said
//! "fetch that fruit" or "take it home"; everything that follows happens here,
//! and the model is asked again only when the intent ends or falls apart.

use bevy::prelude::*;

use crate::config::VISION_RADIUS;
use crate::decisions::ThinkTimer;
use crate::world::fruit::Fruit;
use crate::world::grid::{Dir, Grid, GridPos, Occupancy};
use crate::world::nest::{Nest, Stores};
use crate::world::scent::Scent;

use super::components::{Carrying, Facing, MoveAnim};
use super::movement::{ON_THE_BACK, start_step};

/// What an ant is busy with. Absent means: ready for a new decision.
#[derive(Component, Clone, Copy)]
pub enum Intent {
    /// Keep heading that way for `left` more cells. Searching, in other words.
    Walk { dir: Dir, left: i32 },
    /// Walk to this fruit and pick it up.
    Fetch(Entity),
    /// Carry what you hold to the nest and put it down.
    CarryHome,
    /// Follow the scent uphill, which is the way home. No direction is kept:
    /// the slope is read again at every step, because a trail bends and the one
    /// the model named is only its start.
    FollowScent { left: i32 },
}

/// What the intents work on. Grouped to keep the signature readable.
#[derive(bevy::ecs::system::SystemParam)]
pub struct BoardParts<'w> {
    grid: Res<'w, Grid>,
    nest: Res<'w, Nest>,
    occupancy: ResMut<'w, Occupancy>,
    stores: ResMut<'w, Stores>,
    scent: Res<'w, Scent>,
}

/// The ants pursuing something, one step per finished step animation.
type Busy<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static GridPos,
        &'static mut Facing,
        &'static mut Carrying,
        &'static mut ThinkTimer,
        &'static mut Intent,
    ),
    Without<MoveAnim>,
>;

pub fn pursue_intents(
    mut commands: Commands,
    board: BoardParts,
    mut ants: Busy,
    fruits: Query<&GridPos, With<Fruit>>,
) {
    let BoardParts {
        grid,
        nest,
        mut occupancy,
        mut stores,
        scent,
    } = board;
    let grid = *grid;
    let nest = *nest;

    for (entity, position, mut facing, mut carrying, mut timer, mut intent) in &mut ants {
        match *intent {
            Intent::FollowScent { left } => {
                // Home in sight: from here the ant walks straight there, which
                // is the next decision rather than this one.
                if nest.within(position.0, VISION_RADIUS) {
                    done(&mut commands, entity, &mut timer);
                    continue;
                }

                // The slope is read again every step — the trail may bend.
                let Some((uphill, _)) = scent.uphill(grid, position.0) else {
                    // The peak, or the trail has faded under it. Either way the
                    // ant has to decide anew.
                    done(&mut commands, entity, &mut timer);
                    continue;
                };

                let walked = start_step(
                    &mut commands,
                    &mut occupancy,
                    grid,
                    entity,
                    position.0,
                    uphill,
                    &mut facing,
                );
                if !walked || left <= 1 {
                    done(&mut commands, entity, &mut timer);
                } else {
                    *intent = Intent::FollowScent { left: left - 1 };
                }
            }

            Intent::Walk { dir, left } => {
                let walked = start_step(
                    &mut commands,
                    &mut occupancy,
                    grid,
                    entity,
                    position.0,
                    dir,
                    &mut facing,
                );
                if !walked || left <= 1 {
                    // Blocked, or far enough. No `ask_now` here on purpose:
                    // searching is paced by the interval, while reaching a goal
                    // is worth a question at once.
                    commands.entity(entity).remove::<Intent>();
                } else {
                    *intent = Intent::Walk {
                        dir,
                        left: left - 1,
                    };
                }
            }

            Intent::Fetch(fruit) => {
                // No cell of its own any more: eaten, or already on somebody
                // else's back. The intent is void and the ant asks again.
                let Ok(target) = fruits.get(fruit) else {
                    done(&mut commands, entity, &mut timer);
                    continue;
                };

                if next_to(position.0, target.0) {
                    occupancy.vacate(grid, target.0, fruit);
                    carrying.fruit = Some(fruit);
                    facing.0 = Dir::nearest(target.0 - position.0);
                    // The fruit leaves the board and rides along.
                    commands
                        .entity(fruit)
                        .remove::<GridPos>()
                        .insert((ChildOf(entity), Transform::from_translation(ON_THE_BACK)));
                    done(&mut commands, entity, &mut timer);
                } else {
                    walk(
                        &mut commands,
                        &mut occupancy,
                        grid,
                        entity,
                        position.0,
                        target.0,
                        &mut facing,
                    );
                }
            }

            Intent::CarryHome => {
                let Some(fruit) = carrying.fruit else {
                    done(&mut commands, entity, &mut timer);
                    continue;
                };

                if nest.contains(position.0) {
                    stores.0 += 1;
                    carrying.fruit = None;
                    commands.entity(fruit).despawn();
                    done(&mut commands, entity, &mut timer);
                } else {
                    walk(
                        &mut commands,
                        &mut occupancy,
                        grid,
                        entity,
                        position.0,
                        nest.nearest_cell(position.0),
                        &mut facing,
                    );
                }
            }
        }
    }
}

/// The intent is over, one way or the other: drop it and ask again at once
/// rather than waiting out the interval.
fn done(commands: &mut Commands, ant: Entity, timer: &mut ThinkTimer) {
    commands.entity(ant).remove::<Intent>();
    timer.ask_now();
}

fn next_to(here: IVec2, there: IVec2) -> bool {
    let offset = there - here;
    offset.x.abs().max(offset.y.abs()) <= 1
}

/// One step towards `target`.
///
/// No A*: the board is small and the only obstacles are ants and fruit. The
/// free neighbour closest to the target wins; if none of them is closer, the
/// least bad one is still better than standing still, because whatever is in
/// the way is usually an ant that will have moved on a moment later.
fn walk(
    commands: &mut Commands,
    occupancy: &mut Occupancy,
    grid: Grid,
    ant: Entity,
    from: IVec2,
    target: IVec2,
    facing: &mut Facing,
) {
    let best = Dir::COMPASS
        .into_iter()
        .filter(|direction| occupancy.is_free(grid, from + direction.offset()))
        .min_by_key(|direction| {
            let cell = from + direction.offset();
            let offset = target - cell;
            offset.x.abs().max(offset.y.abs())
        });

    if let Some(direction) = best {
        start_step(commands, occupancy, grid, ant, from, direction, facing);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::*;
    use crate::ants::components::{AntId, SinceNest};
    use crate::ants::movement::{animate_steps, apply_decisions};
    use crate::decisions::{
        Action, ActiveSource, AntMove, AntView, DecisionSource, DecisionStats, Origin,
    };
    use crate::world::grid::Occupant;
    use crate::world::scent::Scent;

    /// A source the test writes into, so the sequence of decisions is exact
    /// instead of waited for.
    #[derive(Clone, Default)]
    struct Script(Arc<Mutex<Vec<AntMove>>>);

    impl DecisionSource for Script {
        fn request(&mut self, _ant: &AntView<'_>) {}

        fn poll(&mut self) -> Vec<AntMove> {
            std::mem::take(&mut self.0.lock().expect("no other thread holds this"))
        }

        fn name(&self) -> &'static str {
            "script"
        }
    }

    struct Fixture {
        app: App,
        script: Script,
        ant: Entity,
        fruit: Entity,
    }

    fn fixture(ant_at: IVec2, fruit_at: IVec2) -> Fixture {
        let mut app = App::new();
        let grid = Grid::default();
        let script = Script::default();

        app.insert_resource(Time::<()>::default())
            .insert_resource(grid)
            .insert_resource(Occupancy::new(grid))
            .insert_resource(crate::world::scent::Scent::new(grid))
            .init_resource::<Nest>()
            .init_resource::<Stores>()
            .init_resource::<DecisionStats>()
            .insert_resource(ActiveSource(Box::new(script.clone())))
            .add_systems(
                Update,
                (apply_decisions, pursue_intents, animate_steps).chain(),
            );

        let ant = app
            .world_mut()
            .spawn((
                AntId(0),
                GridPos(ant_at),
                Facing(Dir::North),
                Carrying::default(),
                SinceNest::default(),
                crate::ants::components::LastDecision::default(),
                ThinkTimer::staggered(0, 1),
                Transform::default(),
            ))
            .id();
        let fruit = app
            .world_mut()
            .spawn((Fruit, GridPos(fruit_at), Transform::default()))
            .id();

        let mut occupancy = app.world_mut().resource_mut::<Occupancy>();
        occupancy.occupy(grid, ant_at, Occupant::Ant(ant));
        occupancy.occupy(grid, fruit_at, Occupant::Fruit(fruit));

        Fixture {
            app,
            script,
            ant,
            fruit,
        }
    }

    impl Fixture {
        fn decide(&mut self, action: Action) {
            self.script
                .0
                .lock()
                .expect("no other thread holds this")
                .push(AntMove {
                    id: AntId(0),
                    action,
                    origin: Origin::Rules,
                });
        }

        /// Lets the simulation work for `seconds`, in 50 ms slices.
        fn run(&mut self, seconds: f32) {
            for _ in 0..((seconds / 0.05) as u32) {
                self.app
                    .world_mut()
                    .resource_mut::<Time>()
                    .advance_by(Duration::from_millis(50));
                self.app.update();
            }
        }

        fn carrying(&self) -> Option<Entity> {
            self.app
                .world()
                .get::<Carrying>(self.ant)
                .expect("the ant has the component")
                .fruit
        }

        fn busy(&self) -> bool {
            self.app.world().get::<Intent>(self.ant).is_some()
        }

        fn stored(&self) -> u32 {
            self.app.world().resource::<Stores>().0
        }
    }

    /// The whole point of 2c: one decision, several cells walked, fruit on the
    /// back — and then one more decision for the way home.
    #[test]
    fn one_decision_fetches_a_fruit_four_cells_away() {
        let mut fixture = fixture(IVec2::new(5, 5), IVec2::new(5, 9));

        fixture.decide(Action::Fetch {
            fruit: fixture.fruit,
            dir: Dir::North,
            distance: 4,
        });
        fixture.run(3.0);

        assert_eq!(fixture.carrying(), Some(fixture.fruit));
        assert!(!fixture.busy(), "the intent is over and the ant asks again");
        assert!(
            fixture.app.world().get::<GridPos>(fixture.fruit).is_none(),
            "the fruit has left the board"
        );
    }

    #[test]
    fn and_one_more_carries_it_home() {
        let mut fixture = fixture(IVec2::new(5, 5), IVec2::new(5, 9));
        fixture.decide(Action::Fetch {
            fruit: fixture.fruit,
            dir: Dir::North,
            distance: 4,
        });
        fixture.run(3.0);

        fixture.decide(Action::CarryHome);
        fixture.run(6.0);

        assert_eq!(fixture.stored(), 1);
        assert_eq!(fixture.carrying(), None, "hands free again");
        assert!(
            fixture.app.world().get_entity(fixture.fruit).is_err(),
            "the fruit is eaten, not lying around"
        );
    }

    /// The point of the pheromones: a laden ant finds its way home to a nest it
    /// cannot see, by walking up the trail the colony laid on its way out.
    #[test]
    fn a_trail_leads_home_to_a_nest_out_of_sight() {
        let nest = Nest::default();
        let ant_at = IVec2::new(20, 12);
        let mut fixture = fixture(ant_at, ant_at + IVec2::Y);
        let grid = *fixture.app.world().resource::<Grid>();

        assert!(
            !nest.within(ant_at, VISION_RADIUS),
            "the nest must be out of sight, or the test proves nothing"
        );

        // Pick the fruit up first — the trail is only offered to a carrier.
        fixture.decide(Action::Fetch {
            fruit: fixture.fruit,
            dir: Dir::North,
            distance: 1,
        });
        fixture.run(0.5);
        assert!(fixture.carrying().is_some(), "picked it up");

        {
            // The way out, as a searching ant would have marked it: faintest far
            // from the nest, strongest close to it.
            let mut scent = fixture.app.world_mut().resource_mut::<Scent>();
            for (steps, cell) in [
                IVec2::new(19, 12),
                IVec2::new(18, 11),
                IVec2::new(17, 11),
                IVec2::new(16, 10),
                IVec2::new(15, 10),
                IVec2::new(14, 9),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, cell)| (5 - index as u32, cell))
            {
                scent.deposit(grid, cell, steps);
            }
        }

        fixture.decide(Action::FollowScent(Dir::West));
        fixture.run(3.0);

        let position = fixture
            .app
            .world()
            .get::<GridPos>(fixture.ant)
            .expect("the ant still has a cell")
            .0;
        assert!(
            nest.within(position, VISION_RADIUS),
            "followed the trail to {position:?} and still cannot see home"
        );
        assert!(
            !fixture.busy(),
            "the intent ends when the nest comes in sight"
        );
    }

    /// Somebody else was quicker. The intent is void, and the ant must not keep
    /// walking towards a fruit that is gone.
    #[test]
    fn an_intent_for_a_vanished_fruit_is_dropped() {
        let mut fixture = fixture(IVec2::new(5, 5), IVec2::new(5, 9));
        fixture.decide(Action::Fetch {
            fruit: fixture.fruit,
            dir: Dir::North,
            distance: 4,
        });
        fixture.run(0.2);
        assert!(fixture.busy(), "on its way");

        fixture.app.world_mut().entity_mut(fixture.fruit).despawn();
        fixture.run(0.5);

        assert!(!fixture.busy());
        assert_eq!(fixture.carrying(), None);
    }

    /// Carrying home with empty hands cannot happen through the options, but the
    /// simulation must not trip over it either.
    #[test]
    fn carrying_home_with_nothing_in_hand_just_ends() {
        let mut fixture = fixture(IVec2::new(5, 5), IVec2::new(5, 9));
        fixture.decide(Action::CarryHome);
        fixture.run(0.5);

        assert!(!fixture.busy());
        assert_eq!(fixture.stored(), 0);
    }
}
