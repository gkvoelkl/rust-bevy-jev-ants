//! What is visible, turned into the options a single ant may choose from.
//!
//! This is the translation point the whole demo rests on: the list is built
//! from the board every time, so the model can only ever name something the ant
//! can actually do. It cannot fetch a fruit that is not there, and it cannot
//! carry home what it is not holding.

use bevy::prelude::*;

use crate::world::grid::{Dir, Grid, Occupancy, Occupant, free_directions};
use crate::world::nest::Nest;
use crate::world::scent::Scent;

use super::Action;

/// Everything this ant can do right now, in a fixed order so that two ants in
/// the same situation produce the very same request.
pub fn available(
    grid: Grid,
    occupancy: &Occupancy,
    nest: Nest,
    scent: &Scent,
    at: IVec2,
    carrying: bool,
    radius: i32,
) -> Vec<Action> {
    let mut options = Vec::new();

    if carrying {
        // Home has to be found, not assumed. Walking straight there is only an
        // option while the nest is in sight; otherwise the ant is on the scent
        // trail or on its own.
        if nest.within(at, radius) {
            options.push(Action::CarryHome);
        }

        // At most one: the slope has one direction, and offering several would
        // be inventing them. Uphill is the way home.
        if let Some((direction, _)) = scent.uphill(grid, at) {
            options.push(Action::FollowScent(direction));
        }
    } else {
        // Food is found by eye. The trail leads home, which is of no use to an
        // ant with empty hands, so it is not offered.
        options.extend(fruits_in_sight(grid, occupancy, at, radius));
    }

    let ways_out = free_directions(grid, occupancy, at);
    if ways_out.is_empty() {
        // Wedged in by neighbours. Now standing still is not a choice, it is
        // the only thing left, and it has to be on the list — an empty choice
        // would be a request with no answer.
        options.push(Action::Wait);
    } else {
        options.extend(ways_out.into_iter().map(Action::Walk));
    }

    options
}

/// The nearest fruit per compass direction, and no more than that.
///
/// Offering every single fruit would mean two options for "north-east" that the
/// model cannot tell apart, and a state full of detail that does not help the
/// question — which the API docs warn costs accuracy.
fn fruits_in_sight(grid: Grid, occupancy: &Occupancy, at: IVec2, radius: i32) -> Vec<Action> {
    let mut nearest: [Option<(Entity, i32)>; 8] = [None; 8];

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let offset = IVec2::new(dx, dy);
            if offset == IVec2::ZERO {
                continue;
            }
            let Some(Occupant::Fruit(fruit)) = occupancy.at(grid, at + offset) else {
                continue;
            };

            let distance = dx.abs().max(dy.abs());
            let direction = Dir::nearest(offset);
            let slot = &mut nearest[direction_index(direction)];
            if slot.is_none_or(|(_, known)| distance < known) {
                *slot = Some((fruit, distance));
            }
        }
    }

    Dir::COMPASS
        .into_iter()
        .filter_map(|direction| {
            nearest[direction_index(direction)].map(|(fruit, distance)| Action::Fetch {
                fruit,
                dir: direction,
                distance,
            })
        })
        .collect()
}

fn direction_index(direction: Dir) -> usize {
    Dir::COMPASS
        .iter()
        .position(|candidate| *candidate == direction)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Grid, Occupancy, Nest) {
        let grid = Grid {
            width: 12,
            height: 12,
        };
        (
            grid,
            Occupancy::new(grid),
            Nest {
                min: IVec2::new(5, 5),
                size: 3,
            },
        )
    }

    fn fruit(id: u32) -> Occupant {
        Occupant::Fruit(Entity::from_raw_u32(id).unwrap())
    }

    fn fetches(options: &[Action]) -> Vec<Action> {
        options
            .iter()
            .copied()
            .filter(|option| matches!(option, Action::Fetch { .. }))
            .collect()
    }

    #[test]
    fn a_fruit_in_sight_can_be_fetched() {
        let (grid, mut occupancy, nest) = setup();
        occupancy.occupy(grid, IVec2::new(1, 3), fruit(1));

        let options = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            IVec2::new(1, 1),
            false,
            3,
        );
        assert_eq!(
            fetches(&options),
            vec![Action::Fetch {
                fruit: Entity::from_raw_u32(1).unwrap(),
                dir: Dir::North,
                distance: 2,
            }]
        );
    }

    #[test]
    fn a_fruit_beyond_the_radius_is_not_offered() {
        let (grid, mut occupancy, nest) = setup();
        occupancy.occupy(grid, IVec2::new(1, 6), fruit(1));

        let options = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            IVec2::new(1, 1),
            false,
            3,
        );
        assert!(fetches(&options).is_empty());
    }

    /// Two fruits in the same direction would give the model two options it
    /// cannot tell apart. Only the nearer one is offered.
    #[test]
    fn only_the_nearest_fruit_per_direction_is_offered() {
        let (grid, mut occupancy, nest) = setup();
        occupancy.occupy(grid, IVec2::new(1, 2), fruit(1));
        occupancy.occupy(grid, IVec2::new(1, 4), fruit(2));

        let options = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            IVec2::new(1, 1),
            false,
            3,
        );
        let offered = fetches(&options);
        assert_eq!(offered.len(), 1);
        assert_eq!(
            offered[0],
            Action::Fetch {
                fruit: Entity::from_raw_u32(1).unwrap(),
                dir: Dir::North,
                distance: 1,
            }
        );
    }

    /// Two hands, one load: a carrying ant is offered the way home, not more
    /// fruit.
    /// Two hands, one load: a carrying ant is offered no more fruit.
    #[test]
    fn a_carrying_ant_is_offered_no_fruit() {
        let (grid, mut occupancy, nest) = setup();
        occupancy.occupy(grid, IVec2::new(1, 2), fruit(1));

        let options = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            IVec2::new(1, 1),
            true,
            3,
        );
        assert!(fetches(&options).is_empty());
    }

    #[test]
    fn an_empty_handed_ant_is_never_offered_the_way_home() {
        let (grid, occupancy, nest) = setup();
        let options = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            IVec2::new(1, 1),
            false,
            3,
        );
        assert!(!options.contains(&Action::CarryHome));
    }

    /// Standing still is offered only when there is genuinely nothing else.
    ///
    /// It used to be on every list, and the colony paid for it: twelve ants
    /// waking up in a four-by-four nest have few free neighbours, so `wait` was
    /// one of two or three options and the model picked it about half the time.
    /// Nobody left the nest. An idle ant standing still is never the right
    /// answer in this game — until tiredness exists and resting becomes a real
    /// choice again.
    #[test]
    fn standing_still_is_the_last_resort_only() {
        let (grid, mut occupancy, nest) = setup();
        let at = IVec2::new(1, 1);

        let options = available(grid, &occupancy, nest, &Scent::new(grid), at, false, 3);
        assert!(!options.contains(&Action::Wait), "there are ways out");

        // Wall it in completely.
        for direction in Dir::COMPASS {
            occupancy.occupy(
                grid,
                at + direction.offset(),
                Occupant::Ant(Entity::from_raw_u32(100).unwrap()),
            );
        }
        let boxed_in = available(grid, &occupancy, nest, &Scent::new(grid), at, false, 3);
        assert_eq!(boxed_in, vec![Action::Wait], "nothing else is possible");
    }
}
