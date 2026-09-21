//! Everything that is meant to be tuned lives here, never inline in a system.

/// Board width in cells. Even on purpose: the nest has an even edge too, and
/// only then does it sit exactly in the middle rather than half a cell off.
pub const GRID_WIDTH: i32 = 24;
/// Board height in cells. Even for the same reason.
pub const GRID_HEIGHT: i32 = 16;

/// Edge length of the nest, a square in the centre of the board. Sixteen cells,
/// which is where the colony starts out — with a little room to spare.
pub const NEST_SIZE: i32 = 4;

/// How many fruits the board is kept topped up to.
pub const FRUIT_TARGET: usize = 14;
/// Seconds between two fruits growing back.
pub const FRUIT_REGROWTH: f32 = 4.0;
/// Edge length of one cell in pixels.
pub const CELL_SIZE: f32 = 32.0;

/// How many ants the colony starts with. They all begin inside the nest and
/// have to leave it first; the four spare cells keep them from being wedged in
/// at the very first tick.
pub const ANT_COUNT: usize = 12;

/// Upper bound between two decisions for one ant. Not the normal case: an ant
/// that finishes an intent asks again at once, and so does every ant when the
/// queen speaks. `ANTS.md` §9 — event-driven questions matter more than a short
/// fixed interval.
///
/// Measured on 2026-09-21 with twelve ants (`internals/konzept-2.md` §7):
/// 1 s cost $0.97/h, 2 s $0.51/h, 6 s $0.16/h — and the delivery rate fell with
/// it, almost exactly in step. Two seconds is the compromise: half the cost of
/// the old pacing, and ants that still look busy.
pub const THINK_INTERVAL: f32 = 2.0;

/// How far an ant walks on one "head that way" decision. Exploring is an intent
/// too, otherwise searching costs a request per cell.
pub const WANDER_CELLS: i32 = 12;
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

/// How much scent a fresh carrier leaves on a cell.
pub const SCENT_DEPOSIT: f32 = 1.0;

/// What is left of that with every further step since the fruit was picked up.
/// This is what makes a trail point somewhere: the scent is strongest where the
/// fruit was, so walking uphill leads to the food.
pub const SCENT_STEP_DECAY: f32 = 0.88;

/// Seconds for a trail to fade to half. Evaporation is not decoration — it is
/// how the colony forgets a source that has run dry.
pub const SCENT_HALF_LIFE: f32 = 25.0;

/// Below this a cell counts as unscented, so old trails stop being offered.
pub const SCENT_THRESHOLD: f32 = 0.05;
