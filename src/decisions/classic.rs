//! The colony without a model: plain rules, no key, no network.
//!
//! This is what has to carry the game on its own (`ANTS.md` rule 5). It is also
//! the fallback for a single ant whenever Jev cannot answer for it, and the
//! other half of the "with Jev / without Jev" switch — which means it has to be
//! good enough to be a fair comparison, not a straw man.
//!
//! The rule is the one real ants follow: carrying something? go home. Food in
//! sight? go get it. Neither? wander.

use rand::seq::IndexedRandom;

use super::{Action, AntMove, AntView, DecisionSource, Origin};

#[derive(Default)]
pub struct ClassicSource {
    ready: Vec<AntMove>,
}

impl DecisionSource for ClassicSource {
    fn request(&mut self, ant: &AntView<'_>) {
        self.ready.push(AntMove {
            id: ant.id,
            action: decide(ant),
            origin: Origin::Classic,
        });
    }

    fn poll(&mut self) -> Vec<AntMove> {
        std::mem::take(&mut self.ready)
    }

    fn name(&self) -> &'static str {
        "classic rules"
    }
}

fn decide(ant: &AntView<'_>) -> Action {
    if let Some(home) = ant
        .options
        .iter()
        .find(|option| matches!(option, Action::CarryHome))
    {
        return *home;
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

    #[test]
    fn with_nothing_to_do_it_wanders() {
        let options = vec![Action::Walk(Dir::North), Action::Wait];
        let decision = decide(&view(options.clone()));
        assert!(options.contains(&decision));
    }
}
