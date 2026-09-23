use bevy::prelude::*;

use crate::decisions::{Action, Exchange};
use crate::world::grid::Dir;

// `GridPos` lives in `world/grid.rs`: it is a position on the board, and fruits
// stand on cells just as ants do.

/// Stable identity of an ant. Decisions are keyed and ordered by it, which is
/// what makes conflict resolution deterministic.
#[derive(Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AntId(pub u32);

/// Where the ant is looking. Purely cosmetic.
#[derive(Component, Clone, Copy)]
pub struct Facing(pub Dir);

/// The fruit this ant carries, riding on its back. `None` means empty-handed.
#[derive(Component, Default)]
pub struct Carrying {
    pub fruit: Option<Entity>,
}

/// Steps since the ant last stood in the nest (`ANTS.md` §2.2).
///
/// The scent it lays shrinks with this number, which is what gives the trail a
/// direction: strongest at the nest, faintest far out. Walking uphill is
/// walking home.
#[derive(Component, Default)]
pub struct SinceNest(pub u32);

/// What this ant last decided and how sure the model was. Only read by the
/// debug layer — the simulation itself does not care.
#[derive(Component, Default)]
pub struct LastDecision {
    pub action: Option<Action>,
    /// `None` means the classic rules decided, which have no confidence.
    pub confidence: Option<f32>,
}

/// The last complete exchange with the model for this ant — everything that was
/// sent and everything that came back.
///
/// Kept per ant rather than in one list because that is how one reads it: click
/// an ant, see what *it* was asked. Refused answers are kept too; the ant did
/// not act on them, but they are the ones worth looking at.
#[derive(Component, Default)]
pub struct LastExchange(pub Option<Exchange>);

/// A step in progress. Present only while the ant is walking, so `Without<MoveAnim>`
/// is the test for "ready for a new decision".
#[derive(Component)]
pub struct MoveAnim {
    pub from: IVec2,
    pub to: IVec2,
    /// Progress from 0 to 1.
    pub t: f32,
}
