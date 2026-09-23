//! What is visible, turned into the options a single ant may choose from.
//!
//! This is the translation point the whole demo rests on: the list is built
//! from the board every time, so the model can only ever name something the ant
//! can actually do. It cannot fetch a fruit that is not there, and it cannot
//! carry home what it is not holding.

use bevy::prelude::*;

use crate::config::PLANK_VISIBLE_FROM;
use crate::world::grid::{Dir, Grid, Occupancy, Occupant, free_directions};
use crate::world::nest::Nest;
use crate::world::scent::Scent;

use super::Action;

/// What the ant has in its hands. One thing at a time — which is most of what
/// decides its options, so it is one value rather than a row of flags that
/// could contradict each other.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hands {
    Empty,
    Fruit,
    /// One end of the plank. Until a second ant takes the other, there is
    /// nothing to do but hold on.
    Plank,
}

/// Everything this ant can do right now, in a fixed order so that two ants in
/// the same situation produce the very same request.
pub fn available(
    grid: Grid,
    occupancy: &Occupancy,
    nest: Nest,
    scent: &Scent,
    at: IVec2,
    hands: Hands,
    radius: i32,
) -> Vec<Action> {
    let mut options = Vec::new();

    // Carrying the plank is its own situation: hands full, and a crossing to
    // reach. Putting it down has to stay on the list, or an ant that took it
    // somewhere useless could never be told to leave it.
    if hands == Hands::Plank {
        return vec![Action::LetGoPlank, Action::Wait];
    }

    if hands == Hands::Fruit {
        // Home has to be found, not assumed. Walking straight there is only an
        // option while the nest is in sight; otherwise the ant is on the scent
        // trail or on its own.
        if nest.within(at, radius) {
            options.push(Action::CarryHome);
        }
    } else {
        // Food is found by eye, and two hands hold one load.
        options.extend(fruits_in_sight(grid, occupancy, at, radius));
        // A plank still on the ground is a plank that needs hands: the moment a
        // second ant takes hold it leaves the board and stops being offered.
        options.extend(plank_in_sight(grid, occupancy, at));
    }

    // The trail is offered to whoever is standing next to one, carrying or not.
    // It used to be for carriers only, on the grounds that the way home is no
    // use to an ant that is still searching. But the queen can call the colony
    // back, and then an empty-handed ant far out knew where home was — the
    // sighting says so — without any way to walk there. A `Walk` runs twelve
    // cells dead straight; a trail bends, and only `FollowScent` reads the
    // slope again at every step. The order was obeyed half way and no further.
    //
    // At most one: the slope has one direction, and offering several would be
    // inventing them.
    //
    // And never while the nest is in sight. `pursue_intents` ends a
    // `FollowScent` on the spot once home is visible — there is nothing left
    // for a trail to add. Offering it there produced an ant that chose it, took
    // no step, asked again at once, chose it again: standing still and paying
    // for a request every round trip. Worse, it was an absorbing state. Every
    // ant that ever followed a trail home ended up parked around the nest, so
    // the colony slowly stopped moving.
    //
    // The rule behind it is the one this whole file exists for: an option the
    // simulation will refuse to carry out is not an option.
    if !nest.within(at, radius)
        && let Some((direction, _)) = scent.uphill(grid, at)
    {
        options.push(Action::FollowScent(direction));
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

/// The plank, if one is lying within sight and still needs hands.
///
/// Found through `Occupancy` rather than a query of its own, and that is not a
/// shortcut: a plank that has been picked up leaves the board, so anything
/// still standing on a cell is by definition one that can still be taken.
fn plank_in_sight(grid: Grid, occupancy: &Occupancy, at: IVec2) -> Option<Action> {
    let mut nearest: Option<(Entity, Dir, i32)> = None;
    let radius = PLANK_VISIBLE_FROM;

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let offset = IVec2::new(dx, dy);
            if offset == IVec2::ZERO {
                continue;
            }
            let Some(Occupant::Plank(plank)) = occupancy.at(grid, at + offset) else {
                continue;
            };
            let distance = dx.abs().max(dy.abs());
            if nearest.is_none_or(|(_, _, known)| distance < known) {
                nearest = Some((plank, Dir::nearest(offset), distance));
            }
        }
    }

    nearest.map(|(plank, dir, distance)| Action::TakePlank {
        plank,
        dir,
        distance,
    })
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

    fn is_trail(option: &Action) -> bool {
        matches!(option, Action::FollowScent(_))
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
            Hands::Empty,
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
            Hands::Empty,
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
            Hands::Empty,
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
            Hands::Fruit,
            3,
        );
        assert!(fetches(&options).is_empty());
    }

    /// Putting a fruit down is a carrier's move. An ant with empty hands is
    /// never offered it, however close to the nest it stands.
    #[test]
    fn an_empty_handed_ant_is_never_offered_carry_home() {
        let (grid, occupancy, nest) = setup();
        let options = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            IVec2::new(1, 1),
            Hands::Empty,
            3,
        );
        assert!(!options.contains(&Action::CarryHome));
    }

    /// The trail, though, is offered to everyone standing beside one — so that
    /// "everybody back to the nest" is an order an empty-handed ant can
    /// actually carry out, and not merely be informed about.
    #[test]
    fn an_empty_handed_ant_is_offered_the_trail() {
        let (grid, occupancy, nest) = setup();
        let at = IVec2::new(1, 1);
        let mut scent = Scent::new(grid);
        // Fresh scent one cell north, nothing underfoot: uphill is north.
        scent.deposit(grid, at + Dir::North.offset(), 0);

        let options = available(grid, &occupancy, nest, &scent, at, Hands::Empty, 3);
        assert!(options.contains(&Action::FollowScent(Dir::North)));
        assert!(
            !options.contains(&Action::CarryHome),
            "the trail is not the same offer as putting a fruit down"
        );
    }

    /// The keys become the `criteria` map, so two options sharing one would
    /// silently collapse into a single entry and the ant would be offered less
    /// than the board allows. Checked on a crowded board rather than in theory.
    #[test]
    fn every_option_has_its_own_key() {
        let (grid, mut occupancy, nest) = setup();
        let at = IVec2::new(1, 1);
        occupancy.occupy(grid, IVec2::new(1, 3), fruit(1));
        occupancy.occupy(grid, IVec2::new(3, 1), fruit(2));
        occupancy.occupy(grid, IVec2::new(3, 3), fruit(3));
        let mut scent = Scent::new(grid);
        scent.deposit(grid, at + Dir::North.offset(), 0);

        let options = available(grid, &occupancy, nest, &scent, at, Hands::Empty, 3);
        let keys: std::collections::BTreeSet<String> =
            options.iter().map(|option| option.key()).collect();

        assert!(options.len() > 5, "the situation has to be a crowded one");
        assert_eq!(
            keys.len(),
            options.len(),
            "two options share a key: {keys:?}"
        );
    }

    /// The trail's key carries no direction: only one is ever offered, and a
    /// `follow_scent_north` next to a plain `north` reads as a variant of it.
    #[test]
    fn the_trail_key_is_the_same_whichever_way_it_leads() {
        assert_eq!(Action::FollowScent(Dir::North).key(), "follow_scent");
        assert_eq!(Action::FollowScent(Dir::SouthWest).key(), "follow_scent");
        assert!(
            Dir::COMPASS
                .iter()
                .all(|d| Action::Walk(*d).key() != "follow_scent"),
            "and it must not collide with a compass step"
        );
    }

    /// A carrier out of sight of the nest has the trail and nothing else.
    #[test]
    fn a_carrier_out_of_sight_is_offered_the_trail() {
        let (grid, occupancy, nest) = setup();
        let at = IVec2::new(1, 1);
        let mut scent = Scent::new(grid);
        scent.deposit(grid, at + Dir::North.offset(), 0);

        assert!(!nest.within(at, 3), "the test needs the nest out of sight");
        let options = available(grid, &occupancy, nest, &scent, at, Hands::Fruit, 3);
        assert!(options.contains(&Action::FollowScent(Dir::North)));
        assert!(!options.contains(&Action::CarryHome));
    }

    /// With home in sight the trail is **not** offered — to anyone.
    ///
    /// `pursue_intents` ends a `FollowScent` immediately once the nest is
    /// visible, so offering it there is offering a move that takes no step. An
    /// ant picked it, went nowhere, asked again at once and picked it again;
    /// ants that came home on a trail piled up around the nest and stopped.
    #[test]
    fn in_sight_of_the_nest_the_trail_is_not_offered() {
        let (grid, occupancy, nest) = setup();
        let at = nest.min - IVec2::ONE;
        let mut scent = Scent::new(grid);
        scent.deposit(grid, at + Dir::South.offset(), 0);

        assert!(nest.within(at, 3), "the test needs the nest in sight");

        let carrying = available(grid, &occupancy, nest, &scent, at, Hands::Fruit, 3);
        assert!(
            carrying.contains(&Action::CarryHome),
            "walking in is the move here"
        );
        assert!(!carrying.iter().any(is_trail));

        let searching = available(grid, &occupancy, nest, &scent, at, Hands::Empty, 3);
        assert!(!searching.iter().any(is_trail));
        assert!(
            searching.iter().any(|o| matches!(o, Action::Walk(_))),
            "it can still walk, which is what gets it into the nest"
        );
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

        let options = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            at,
            Hands::Empty,
            3,
        );
        assert!(!options.contains(&Action::Wait), "there are ways out");

        // Wall it in completely.
        for direction in Dir::COMPASS {
            occupancy.occupy(
                grid,
                at + direction.offset(),
                Occupant::Ant(Entity::from_raw_u32(100).unwrap()),
            );
        }
        let boxed_in = available(
            grid,
            &occupancy,
            nest,
            &Scent::new(grid),
            at,
            Hands::Empty,
            3,
        );
        assert_eq!(boxed_in, vec![Action::Wait], "nothing else is possible");
    }
}
