use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::config::{STEP_DURATION, WANDER_CELLS};
use crate::decisions::{Action, ActiveSource, DecisionStats, Origin};
use crate::world::grid::{Dir, Grid, GridPos, Occupancy, Occupant};
use crate::world::scent::Scent;

use crate::world::nest::Nest;

use super::components::{AntId, Carrying, Facing, LastDecision, MoveAnim, SinceNest};
use super::intent::Intent;

/// Where a carried fruit sits: on the ant's back, just behind its middle. The
/// fruit is a child of the ant, so it turns with it.
pub(super) const ON_THE_BACK: Vec3 = Vec3::new(0.0, -4.0, 0.2);

/// Begins a step into `direction`, if that cell is still free.
///
/// The target is reserved now and the old cell released only when the step
/// ends. That rules out both overlapping and two ants swapping through each
/// other.
pub(super) fn start_step(
    commands: &mut Commands,
    occupancy: &mut Occupancy,
    grid: Grid,
    ant: Entity,
    from: IVec2,
    direction: Dir,
    facing: &mut Facing,
) -> bool {
    let target = from + direction.offset();
    if !occupancy.is_free(grid, target) {
        return false;
    }

    occupancy.occupy(grid, target, Occupant::Ant(ant));
    facing.0 = direction;
    commands.entity(ant).insert(MoveAnim {
        from,
        to: target,
        t: 0.0,
    });
    true
}

/// The ants that are ready for a new decision — the ones not mid-step.
type ReadyAnts<'w, 's> =
    Query<'w, 's, (Entity, &'static AntId, &'static mut LastDecision), Without<MoveAnim>>;

/// Takes whatever the decision source has ready and hands it to the simulation.
///
/// Nothing is carried out here any more: every action is an intent, and
/// `intent::pursue_intents` works on it over the following ticks. What does
/// happen here is the bookkeeping — the decision is recorded for the debug
/// layer and counted in the statistics, whether or not the ant still exists to
/// act on it.
pub fn apply_decisions(
    mut commands: Commands,
    mut source: ResMut<ActiveSource>,
    mut stats: ResMut<DecisionStats>,
    mut ants: ReadyAnts,
) {
    stats.discarded = source.0.discarded();

    let mut moves = source.0.poll();
    if moves.is_empty() {
        return;
    }
    // Ascending ant id, so two ants that want the same thing always resolve the
    // same way.
    moves.sort_by_key(|ant_move| ant_move.id);

    let by_id: HashMap<AntId, Entity> = ants.iter().map(|(entity, id, _)| (*id, entity)).collect();

    for ant_move in moves {
        // Counted before anything else: an answer that arrives too late still
        // cost what it cost.
        stats.record(&ant_move.origin);

        let Some(&entity) = by_id.get(&ant_move.id) else {
            continue;
        };
        let Ok((_, _, mut last)) = ants.get_mut(entity) else {
            continue;
        };

        last.action = Some(ant_move.action);
        last.confidence = match ant_move.origin {
            Origin::Jev { confidence, .. } => Some(confidence),
            Origin::Rules => None,
        };

        match ant_move.action {
            Action::Wait => {
                commands.entity(entity).remove::<Intent>();
            }
            Action::Walk(direction) => {
                commands.entity(entity).insert(Intent::Walk {
                    dir: direction,
                    left: WANDER_CELLS,
                });
            }
            Action::Fetch { fruit, .. } => {
                commands.entity(entity).insert(Intent::Fetch(fruit));
            }
            Action::CarryHome => {
                commands.entity(entity).insert(Intent::CarryHome);
            }
            Action::TakePlank { plank, .. } => {
                commands.entity(entity).insert(Intent::TakePlank(plank));
            }
            Action::LetGoPlank => {
                // The plank is put down by `pursue_intents`, which owns the
                // board. Dropping the intent is how the ant says so.
                commands.entity(entity).remove::<Intent>();
            }
            Action::FollowScent(_) => {
                // The direction is not carried over: the intent reads the slope
                // again at every step.
                commands
                    .entity(entity)
                    .insert(Intent::FollowScent { left: WANDER_CELLS });
            }
        }
    }
}

pub fn animate_steps(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<Grid>,
    nest: Res<Nest>,
    mut occupancy: ResMut<Occupancy>,
    mut scent: ResMut<Scent>,
    mut ants: Query<(
        Entity,
        &mut MoveAnim,
        &mut GridPos,
        &mut Transform,
        &Carrying,
        &mut SinceNest,
    )>,
) {
    let grid = *grid;
    let nest = *nest;
    for (entity, mut anim, mut position, mut transform, carrying, mut since_nest) in &mut ants {
        anim.t = (anim.t + time.delta_secs() / STEP_DURATION).min(1.0);

        let from = grid.to_screen(anim.from);
        let to = grid.to_screen(anim.to);
        let eased = anim.t * anim.t * (3.0 - 2.0 * anim.t);
        let position_now = from.lerp(to, eased);
        transform.translation.x = position_now.x;
        transform.translation.y = position_now.y;

        if anim.t >= 1.0 {
            occupancy.vacate(grid, anim.from, entity);
            position.0 = anim.to;

            // How far from home, counted in steps. Standing in the nest resets
            // it — that is where the trail is strongest.
            if nest.contains(anim.to) {
                since_nest.0 = 0;
            } else {
                since_nest.0 += 1;
            }

            // Only a searching ant marks the ground; a carrier has its hands
            // full and follows what was laid on the way out.
            if carrying.fruit.is_none() {
                scent.deposit(grid, anim.to, since_nest.0);
            }

            commands.entity(entity).remove::<MoveAnim>();
        }
    }
}

pub fn face_direction(mut ants: Query<(&Facing, &mut Transform), Changed<Facing>>) {
    for (facing, mut transform) in &mut ants {
        transform.rotation = Quat::from_rotation_z(facing.0.angle());
    }
}
