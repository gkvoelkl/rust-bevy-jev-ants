//! Plain rules — **not a way to play the game.**
//!
//! The game is for learning what Jev does, so nothing here ever steps in for
//! the model: no fallback when a request fails, and nothing at all without a
//! key. A colony that keeps working on rules would hide exactly what one wants
//! to see.
//!
//! What is left are two jobs that are not the game:
//!
//! * the **offline tests** need some decision source, so that sixty tests run
//!   without a key, without the network and without cost;
//! * `--compare` needs a **baseline**, because a number with nothing to compare
//!   it against says little.
//!
//! The rule is the one real ants follow: carrying something and home in sight?
//! walk in. Carrying and there is a trail? follow it home. Empty-handed with
//! food in sight? go get it. None of those? wander.

use rand::seq::IndexedRandom;

use super::{Action, AntMove, AntView, DecisionSource, Origin};

#[derive(Default)]
pub struct RuleSource {
    ready: Vec<AntMove>,
}

impl DecisionSource for RuleSource {
    fn request(&mut self, ant: &AntView<'_>) {
        self.ready.push(AntMove {
            id: ant.id,
            action: decide(ant),
            origin: Origin::Rules,
        });
    }

    fn poll(&mut self) -> Vec<AntMove> {
        std::mem::take(&mut self.ready)
    }

    fn name(&self) -> &'static str {
        "plain rules (test baseline)"
    }
}

fn decide(ant: &AntView<'_>) -> Action {
    // Straight in, if the nest is there to be seen.
    if let Some(home) = ant
        .options
        .iter()
        .find(|option| matches!(option, Action::CarryHome))
    {
        return *home;
    }

    // Otherwise the trail home, which is only ever offered to a carrier.
    if let Some(trail) = ant
        .options
        .iter()
        .find(|option| matches!(option, Action::FollowScent(_)))
    {
        return *trail;
    }

    // The nearest fruit, and on a tie the one that comes first in the option
    // list — which is fixed, so the rules stay reproducible.
    let nearest = ant
        .options
        .iter()
        .filter_map(|option| match option {
            Action::Fetch { distance, .. } => Some((*distance, *option)),
            _ => None,
        })
        .min_by_key(|(distance, _)| *distance);
    if let Some((_, fetch)) = nearest {
        return fetch;
    }

    let mut rng = rand::rng();
    ant.options
        .choose(&mut rng)
        .copied()
        .unwrap_or(Action::Wait)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ants::components::AntId;
    use crate::world::grid::Dir;
    use bevy::prelude::Entity;

    fn view(options: Vec<Action>) -> AntView<'static> {
        AntView {
            id: AntId(0),
            order: "",
            options,
            sightings: Vec::new(),
            carrying: false,
        }
    }

    fn fetch(id: u32, dir: Dir, distance: i32) -> Action {
        Action::Fetch {
            fruit: Entity::from_raw_u32(id).unwrap(),
            dir,
            distance,
        }
    }

    #[test]
    fn carrying_beats_everything() {
        let decision = decide(&view(vec![
            Action::CarryHome,
            Action::Walk(Dir::North),
            Action::Wait,
        ]));
        assert_eq!(decision, Action::CarryHome);
    }

    #[test]
    fn the_nearest_fruit_wins() {
        let far = fetch(1, Dir::North, 3);
        let near = fetch(2, Dir::South, 1);
        let decision = decide(&view(vec![far, near, Action::Wait]));
        assert_eq!(decision, near);
    }

    /// Seeing the nest beats following a trail towards it.
    #[test]
    fn the_nest_in_sight_beats_the_trail() {
        let decision = decide(&view(vec![
            Action::FollowScent(Dir::West),
            Action::CarryHome,
            Action::Wait,
        ]));
        assert_eq!(decision, Action::CarryHome);
    }

    /// Out of sight of the nest, the trail is what a carrier has.
    #[test]
    fn a_carrier_out_of_sight_follows_the_trail() {
        let decision = decide(&view(vec![
            Action::Walk(Dir::North),
            Action::FollowScent(Dir::West),
            Action::Wait,
        ]));
        assert_eq!(decision, Action::FollowScent(Dir::West));
    }

    #[test]
    fn with_nothing_to_do_it_wanders() {
        let options = vec![Action::Walk(Dir::North), Action::Wait];
        let decision = decide(&view(options.clone()));
        assert!(options.contains(&decision));
    }
}
