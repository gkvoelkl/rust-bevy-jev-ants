//! The source that needs no model and no key: a random walk over the free cells.
//!
//! It keeps the game alive without an API and is the seed of the classic mode
//! that later has to carry the whole colony on its own. It answers immediately,
//! which is allowed — `poll` only promises that the caller is never blocked.

use rand::seq::IndexedRandom;

use crate::world::grid::Dir;

use super::{AntMove, AntView, DecisionSource, Origin};

#[derive(Default)]
pub struct RandomSource {
    ready: Vec<AntMove>,
}

impl DecisionSource for RandomSource {
    fn request(&mut self, ant: &AntView<'_>) {
        let mut rng = rand::rng();
        self.ready.push(AntMove {
            id: ant.id,
            dir: ant.options.choose(&mut rng).copied().unwrap_or(Dir::Stay),
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
