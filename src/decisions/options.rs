//! What is visible, turned into the options a single ant may choose from.
//!
//! This is the translation point the whole demo rests on: the list is built
//! from the board every time, so the model can only ever name something the ant
//! can actually do. It cannot fetch a fruit that is not there, and it cannot
//! carry home what it is not holding.

use bevy::prelude::*;

use crate::world::grid::{Dir, Grid, Occupancy, Occupant, free_directions};
use crate::world::nest::Nest;

use super::Action;

/// Everything this ant can do right now, in a fixed order so that two ants in
/// the same situation produce the very same request.
pub fn available(
    grid: Grid,
    occupancy: &Occupancy,
    nest: Nest,
    at: IVec2,
    carrying: bool,
    radius: i32,
) -> Vec<Action> {
    let mut options = Vec::new();

    if carrying {
        // An ant knows the way home even when it cannot see the nest — real
        // ones navigate by path integration. Fruit, on the other hand, has to
        // be seen to be fetched.
        options.push(Action::CarryHome);
    } else {
        options.extend(fruits_in_sight(grid, occupancy, at, radius));
    }

    options.extend(
        free_directions(grid, occupancy, at)
            .into_iter()
            .filter(|direction| *direction != Dir::Stay)
            .map(Action::Walk),
    );
    options.push(Action::Wait);

    let _ = nest; // the nest only matters for CarryHome, which needs no cell
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

        let options = available(grid, &occupancy, nest, IVec2::new(1, 1), false, 3);
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

        let options = available(grid, &occupancy, nest, IVec2::new(1, 1), false, 3);
        assert!(fetches(&options).is_empty());
    }

    /// Two fruits in the same direction would give the model two options it
    /// cannot tell apart. Only the nearer one is offered.
    #[test]
    fn only_the_nearest_fruit_per_direction_is_offered() {
        let (grid, mut occupancy, nest) = setup();
        occupancy.occupy(grid, IVec2::new(1, 2), fruit(1));
        occupancy.occupy(grid, IVec2::new(1, 4), fruit(2));

        let options = available(grid, &occupancy, nest, IVec2::new(1, 1), false, 3);
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
    #[test]
    fn a_carrying_ant_is_offered_the_way_home_and_no_fruit() {
        let (grid, mut occupancy, nest) = setup();
        occupancy.occupy(grid, IVec2::new(1, 2), fruit(1));

        let options = available(grid, &occupancy, nest, IVec2::new(1, 1), true, 3);
        assert!(options.contains(&Action::CarryHome));
        assert!(fetches(&options).is_empty());
    }

    #[test]
    fn an_empty_handed_ant_is_never_offered_the_way_home() {
        let (grid, occupancy, nest) = setup();
        let options = available(grid, &occupancy, nest, IVec2::new(1, 1), false, 3);
        assert!(!options.contains(&Action::CarryHome));
    }

    /// Standing still is always on the list, so there is never an empty choice.
    #[test]
    fn standing_still_is_always_offered() {
        let (grid, occupancy, nest) = setup();
        let options = available(grid, &occupancy, nest, IVec2::new(1, 1), false, 3);
        assert!(options.contains(&Action::Wait));
    }
}
