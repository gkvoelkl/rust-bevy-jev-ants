pub mod components;
pub mod movement;

use bevy::prelude::*;
use rand::seq::{IndexedRandom, SliceRandom};

use crate::config::{ANT_COUNT, CELL_SIZE};
use crate::decisions::{ThinkSet, ThinkTimer};
use crate::world::grid::{Dir, Grid, Occupancy};
use crate::world::render::Z_ANT;

use components::{AntId, Facing, GridPos, LastDecision};

pub const BODY: Color = Color::srgb(0.85, 0.42, 0.18);
const HEAD: Color = Color::srgb(0.96, 0.72, 0.36);

pub struct AntsPlugin;

impl Plugin for AntsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_ants).add_systems(
            Update,
            (
                movement::apply_decisions,
                movement::animate_steps,
                movement::face_direction,
            )
                .chain()
                .after(ThinkSet),
        );
    }
}

fn spawn_ants(mut commands: Commands, grid: Res<Grid>, mut occupancy: ResMut<Occupancy>) {
    let grid = *grid;
    let mut rng = rand::rng();

    // Draw distinct cells, so no two ants ever start on top of each other.
    let mut cells: Vec<IVec2> = grid.cells().collect();
    cells.shuffle(&mut rng);

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
                Sprite::from_color(BODY, Vec2::new(CELL_SIZE * 0.34, CELL_SIZE * 0.56)),
                Transform::from_translation(grid.to_screen(cell).extend(Z_ANT))
                    .with_rotation(Quat::from_rotation_z(facing.angle())),
                children![(
                    Sprite::from_color(HEAD, Vec2::splat(CELL_SIZE * 0.2)),
                    Transform::from_xyz(0.0, CELL_SIZE * 0.3, 0.1),
                )],
            ))
            .id();
        occupancy.occupy(grid, cell, ant);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decisions::DecisionsPlugin;
    use crate::world::WorldPlugin;
    use bevy::platform::collections::HashSet;
    use components::{GridPos, MoveAnim};
    use std::time::Duration;

    /// Drives the whole simulation headless. `Time` is advanced by hand, so a
    /// simulated minute costs milliseconds and the result does not depend on how
    /// fast the machine runs.
    fn run_headless(frames: usize, step: Duration) {
        let mut app = App::new();
        // No TimePlugin on purpose — the test owns the clock.
        app.insert_resource(Time::<()>::default());
        app.add_plugins((WorldPlugin, DecisionsPlugin::default(), AntsPlugin));
        app.finish();
        app.cleanup();

        for frame in 0..frames {
            app.world_mut().resource_mut::<Time>().advance_by(step);
            app.update();

            let mut taken: HashSet<IVec2> = HashSet::new();
            let mut query = app.world_mut().query::<(&GridPos, Option<&MoveAnim>)>();
            for (position, moving) in query.iter(app.world()) {
                assert!(
                    taken.insert(position.0),
                    "two ants share cell {:?} in frame {frame}",
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

    #[test]
    fn every_ant_stays_on_the_board() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default());
        app.add_plugins((WorldPlugin, DecisionsPlugin::default(), AntsPlugin));
        app.finish();
        app.cleanup();

        for _ in 0..1200 {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_millis(50));
            app.update();
        }

        let grid = *app.world().resource::<Grid>();
        let mut query = app.world_mut().query::<&GridPos>();
        let positions: Vec<IVec2> = query.iter(app.world()).map(|p| p.0).collect();
        assert_eq!(positions.len(), ANT_COUNT);
        for position in positions {
            assert!(grid.contains(position), "{position:?} is off the board");
        }
    }
}
