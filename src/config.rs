//! Everything that is meant to be tuned lives here, never inline in a system.

/// Board width in cells.
///
/// 64 × 32 = **2048** cells, and both edges are even — which is the condition
/// for the even-edged nest to sit exactly in the middle rather than half a cell
/// off. The nest is visible from about 5 % of the board (100 of 2048 cells lie
/// within sight of it), so the scent trail is load-bearing here rather than
/// decoration; on the old 24 × 16 it was a quarter.
pub const GRID_WIDTH: i32 = 64;
/// Board height in cells. Even for the same reason.
pub const GRID_HEIGHT: i32 = 32;

/// Edge length of the nest, a square in the centre of the board.
///
/// Six, so 36 cells — the whole colony starts inside with room to spare, and
/// the ants are not wedged together at the first tick. Even, so it still sits
/// exactly in the middle of an even board (29…34, 13…18).
pub const NEST_SIZE: i32 = 6;

/// How many fruits the board is kept topped up to. About one per 85 cells:
/// enough that the colony finds something, sparse enough that searching costs
/// something.
pub const FRUIT_TARGET: usize = 24;
/// Seconds between two fruits growing back.
pub const FRUIT_REGROWTH: f32 = 4.0;
/// Edge length of one cell in pixels.
///
/// 20 px is about the floor: the debug layer has to print `fetch north-west
/// 0.88` above every ant, and that is what limits how small a cell may get —
/// not the ants. 64 × 20 = 1280 px of board, which still fits a window
/// alongside the bar and the HUD.
pub const CELL_SIZE: f32 = 20.0;

/// How many ants the colony starts with. They all begin inside the nest and
/// have to leave it first.
///
/// Twenty is what the rate limit allows at a two second interval: 600 requests
/// a minute, half of the 1200 the API permits, and about $0.90 an hour. More
/// ants only work with a longer interval.
pub const ANT_COUNT: usize = 20;

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

/// How long a plank carrier may make no headway before giving up.
///
/// Counted in ticks of `pursue_intents`, and the rate is deliberately uneven: a
/// blocked ant has no step animation, so it is looked at every frame and uses
/// these up in about five seconds, while one that is still moving is looked at
/// four times a second and would need a minute of going nowhere. Both are the
/// behaviour wanted — get unstuck quickly, do not abandon a long haul.
///
/// Every other intent ends by itself: a wander runs out of cells, a fetch ends
/// when the fruit is gone, a trail ends at the nest. Carrying had no such end
/// and could therefore last for ever, which is exactly what it did when idle
/// ants filled the only way through.
pub const PLANK_GIVE_UP_TICKS: u32 = 300;

/// How far off a plank is noticed, in cells.
///
/// Further than everything else on purpose. A plank is a large object where a
/// fruit is a crumb, and more to the point: hunting for one cell with three
/// cells of sight is a search, and searching is not what this board is about.
/// The puzzle is getting **two** ants to it at once, and that only begins once
/// they can see it.
pub const PLANK_VISIBLE_FROM: i32 = 8;

/// How far an ant can see, in cells, in every direction. This is the whole
/// reason for one request per ant: the state must never be more than this.
pub const VISION_RADIUS: i32 = 3;

/// Upper bound on requests in the air at once. Reached means an ant does not act
/// this round rather than queueing up.
///
/// Above the ant count on purpose: a new order asks every ant at once, and that
/// burst should fit rather than lose a few ants' reactions to the cap.
pub const MAX_IN_FLIGHT: usize = 24;

/// Below this confidence the model's answer is dropped and the ant does not act
/// this round. Nothing steps in for it — there is no fallback (`ANTS.md` §3.2).
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

/// How many thrown-away answers the log keeps. Enough to see a pattern, few
/// enough that the panel stays readable while the colony runs.
pub const REMEMBERED_DISCARDS: usize = 6;

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
