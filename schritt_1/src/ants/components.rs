use bevy::prelude::*;

use crate::world::grid::Dir;

/// Stable identity of an ant. Decisions are keyed and ordered by it, which is
/// what makes conflict resolution deterministic.
#[derive(Component, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AntId(pub u32);

/// The cell the ant belongs to. While a step is running this still names the
/// cell it started from; `MoveAnim` holds the target.
#[derive(Component, Clone, Copy)]
pub struct GridPos(pub IVec2);

/// Where the ant is looking. Purely cosmetic.
#[derive(Component, Clone, Copy)]
pub struct Facing(pub Dir);

/// What this ant last decided and how sure the model was. Only read by the
/// debug layer — the simulation itself does not care.
#[derive(Component, Default)]
pub struct LastDecision {
    pub dir: Option<Dir>,
    /// `None` means the classic rules decided, which have no confidence.
    pub confidence: Option<f32>,
}

/// A step in progress. Present only while the ant is walking, so `Without<MoveAnim>`
/// is the test for "ready for a new decision".
#[derive(Component)]
pub struct MoveAnim {
    pub from: IVec2,
    pub to: IVec2,
    /// Progress from 0 to 1.
    pub t: f32,
}
