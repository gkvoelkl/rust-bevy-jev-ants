//! Turning a decision into a step, and the step into motion.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::config::STEP_DURATION;
use crate::decisions::{ActiveSource, DecisionStats, Origin};
use crate::world::grid::{Dir, Grid, Occupancy};

use super::components::{AntId, Facing, GridPos, LastDecision, MoveAnim};

/// Picks up whatever the decision source has ready and starts the steps.
///
/// Every move is validated **again** here: between asking and answering a cell
/// may have been taken. Moves are applied in ascending ant id, so two ants that
/// want the same cell always resolve the same way — the lower id wins.
pub fn apply_decisions(
    mut commands: Commands,
    grid: Res<Grid>,
    mut occupancy: ResMut<Occupancy>,
    mut source: ResMut<ActiveSource>,
    mut stats: ResMut<DecisionStats>,
    mut ants: Query<(Entity, &AntId, &GridPos, &mut Facing, &mut LastDecision), Without<MoveAnim>>,
) {
    let mut moves = source.0.poll();
    if moves.is_empty() {
        return;
    }
    moves.sort_by_key(|ant_move| ant_move.id);

    let grid = *grid;
    let by_id: HashMap<AntId, Entity> = ants.iter().map(|(entity, id, ..)| (*id, entity)).collect();

    for ant_move in moves {
        // Counted before anything else: an answer that arrives too late still
        // cost what it cost.
        stats.record(&ant_move.origin);

        let Some(&entity) = by_id.get(&ant_move.id) else {
            continue;
        };
        let Ok((_, _, position, mut facing, mut last)) = ants.get_mut(entity) else {
            continue;
        };

        last.dir = Some(ant_move.dir);
        last.confidence = match ant_move.origin {
            Origin::Jev { confidence, .. } => Some(confidence),
            Origin::Classic => None,
        };

        if ant_move.dir == Dir::Stay {
            continue;
        }

        let target = position.0 + ant_move.dir.offset();
        if !occupancy.is_free(grid, target) {
            continue; // the cell went away while we were waiting — stay put
        }

        // Reserve the target now, release the old cell only when the step ends.
        // That rules out both overlapping and two ants swapping through each other.
        occupancy.occupy(grid, target, entity);
        facing.0 = ant_move.dir;
        commands.entity(entity).insert(MoveAnim {
            from: position.0,
            to: target,
            t: 0.0,
        });
    }
}

pub fn animate_steps(
    mut commands: Commands,
    time: Res<Time>,
    grid: Res<Grid>,
    mut occupancy: ResMut<Occupancy>,
    mut ants: Query<(Entity, &mut MoveAnim, &mut GridPos, &mut Transform)>,
) {
    let grid = *grid;
    for (entity, mut anim, mut position, mut transform) in &mut ants {
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
            commands.entity(entity).remove::<MoveAnim>();
        }
    }
}

pub fn face_direction(mut ants: Query<(&Facing, &mut Transform), Changed<Facing>>) {
    for (facing, mut transform) in &mut ants {
        transform.rotation = Quat::from_rotation_z(facing.0.angle());
    }
}
