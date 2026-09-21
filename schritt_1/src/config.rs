//! Everything that is meant to be tuned lives here, never inline in a system.

/// Board width in cells.
pub const GRID_WIDTH: i32 = 24;
/// Board height in cells.
pub const GRID_HEIGHT: i32 = 16;
/// Edge length of one cell in pixels.
pub const CELL_SIZE: f32 = 32.0;

/// How many ants the colony starts with.
pub const ANT_COUNT: usize = 12;

/// Seconds between two decision rounds.
pub const THINK_INTERVAL: f32 = 1.0;
/// Seconds an ant takes to walk from one cell to the next.
pub const STEP_DURATION: f32 = 0.25;

/// How far an ant can see, in cells, in every direction. This is the whole
/// reason for one request per ant: the state must never be more than this.
pub const VISION_RADIUS: i32 = 3;

/// Upper bound on requests in the air at once. Reached means an ant skips this
/// round rather than queueing up.
pub const MAX_IN_FLIGHT: usize = 16;

/// Below this confidence the model's answer is dropped and the ant falls back to
/// the classic rules.
///
/// Measured on 2026-09-21 (`cargo run -- --measure`, five runs), not guessed.
/// The API's `confidence` is the distance from pure chance, already normalised
/// by the number of options — which matters here because that number differs
/// per ant: one in a corner has four ways out, one in the open has nine.
///
/// Two groups came out of the measurement:
///   * an order with a real preference never scored below 0.29
///     (weakest: "Verteilt euch", 0.29 to 0.36 across runs)
///   * a flat distribution — no order at all — never scored above 0.19
///
/// 0.22 sits in that gap, leaning towards accepting: an order of the queen's
/// that is silently dropped is a worse failure than an ant that stands still
/// for a second. `ANTS.md` §4.3 proposed 0.5, which rejected every order that
/// did not simply name a compass direction.
pub const MIN_CONFIDENCE: f32 = 0.22;

/// How many past orders the input bar keeps on screen.
pub const REMEMBERED_ORDERS: usize = 5;
