//! Where an ant's intent comes from.
//!
//! One request per ant, on purpose. An ant decides from what it can see, and it
//! can see three cells in every direction — nothing more. Putting several ants
//! into one request would defeat that: Jev reads the whole state once and every
//! question is evaluated against all of it, so ants sharing a request would
//! share their sight.
//!
//! Every source is asked without blocking and answers whenever it is ready —
//! the frame loop never waits. In step 1 there are two sources; replay and the
//! classic rules dock onto the same trait later.

pub mod jev;
pub mod options;
pub mod rules;

use std::time::Duration;

use bevy::prelude::*;

use crate::ants::components::{AntId, Carrying, MoveAnim};
use crate::ants::intent::Intent;
use crate::api::Client;
use crate::config::{THINK_INTERVAL, VISION_RADIUS};
use crate::world::grid::{Dir, Grid, GridPos, Occupancy};
use crate::world::nest::Nest;
use crate::world::scent::Scent;
use crate::world::vision::sightings;

/// What an ant can decide to do. Two of these are **intents**, not steps: the
/// simulation carries them out over several ticks and several cells, and the
/// model is asked again only when one finishes or falls apart.
///
/// That is `ANTS.md` rule 3, and it is what makes the game affordable. Fetching
/// a fruit three cells away used to cost three decisions; now it costs one.
/// Picking up and putting down are no longer choices — they are what happens
/// when an intent reaches its end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    /// Head that way for a few cells. Exploring is an intent as well — a single
    /// step per decision is what made searching expensive.
    Walk(Dir),
    /// Stand still this round.
    Wait,
    /// Walk to this fruit and pick it up. Only ever offered for a fruit the ant
    /// can actually see.
    Fetch {
        fruit: Entity,
        /// Where it lies, for the option text and for the classic rules.
        dir: Dir,
        distance: i32,
    },
    /// Carry what you hold back to the nest and put it down. Only offered to an
    /// ant that is carrying something.
    CarryHome,
    /// Follow somebody else's trail uphill, towards where they picked fruit up.
    /// Only offered when there really is a trail next door.
    FollowScent(Dir),
}

impl Action {
    /// The key this action gets in a `choice` question, and what the answer is
    /// read back through. At most one fruit per direction is ever offered, so
    /// these stay unique.
    pub fn key(self) -> String {
        match self {
            Action::Walk(direction) => direction.key().to_string(),
            Action::Wait => "stay".to_string(),
            Action::Fetch { dir, .. } => format!("fetch_{}", dir.key()),
            Action::CarryHome => "carry_home".to_string(),
            Action::FollowScent(direction) => format!("follow_scent_{}", direction.key()),
        }
    }

    /// The option description. These texts move into `assets/questions.ron`
    /// later — rewording them is the real work.
    pub fn description(self) -> String {
        match self {
            Action::Wait => "Stay where you are for now".to_string(),
            Action::Walk(direction) => format!("Head {} for a few cells", direction.spoken()),
            Action::Fetch { dir, distance, .. } => {
                let cells = if distance == 1 { "cell" } else { "cells" };
                format!("Fetch the fruit {distance} {cells} to the {}", dir.spoken())
            }
            Action::CarryHome => "Carry the fruit back to the nest".to_string(),
            Action::FollowScent(direction) => format!(
                "Follow the scent trail {} — another ant carried fruit along it",
                direction.spoken()
            ),
        }
    }

    /// Short form for the debug layer above the ant.
    pub fn label(self) -> String {
        match self {
            Action::Walk(direction) => direction.spoken().to_string(),
            Action::Wait => "wait".to_string(),
            Action::Fetch { dir, .. } => format!("fetch {}", dir.spoken()),
            Action::CarryHome => "home".to_string(),
            Action::FollowScent(direction) => format!("scent {}", direction.spoken()),
        }
    }
}

/// What one ant knows and may do right now. This is the whole world as far as
/// the decision is concerned — no board size, no coordinates, no other ants
/// beyond what it can see.
pub struct AntView<'a> {
    pub id: AntId,
    /// The queen's order, passed through word for word. Empty means none.
    pub order: &'a str,
    /// Generated at runtime from what this ant can reach — never a hardcoded list.
    pub options: Vec<Action>,
    /// Sentences describing what is within sight.
    pub sightings: Vec<String>,
    pub carrying: bool,
}

/// One ant's decision, together with where it came from. The origin is what
/// makes the HUD honest: a step the classic rules took costs nothing and has no
/// confidence, and it must not be counted as if the model had decided it.
pub struct AntMove {
    pub id: AntId,
    pub action: Action,
    pub origin: Origin,
}

pub enum Origin {
    Jev {
        confidence: f32,
        latency_ms: u32,
        input_tokens: u32,
    },
    /// The rule baseline. Never happens while playing.
    Rules,
}

pub trait DecisionSource: Send + Sync {
    /// Asks for a single ant. Must return immediately; the answer arrives
    /// through `poll`.
    fn request(&mut self, ant: &AntView<'_>);
    /// Everything that has arrived since the last call. Empty is the normal case.
    fn poll(&mut self) -> Vec<AntMove>;
    /// For the HUD, so the player can see what is steering the colony.
    fn name(&self) -> &'static str;
    /// Answers thrown away and questions that never went out. Shown rather than
    /// hidden: with nothing standing in for the model, a discarded answer means
    /// an ant did not act, and that should be visible.
    fn discarded(&self) -> u32 {
        0
    }
}

/// What the colony has cost and how fast it thinks. Fed by every decision that
/// comes in, so the numbers are measured rather than estimated.
#[derive(Resource, Default)]
pub struct DecisionStats {
    pub from_jev: u32,
    /// Answers thrown away: too unsure, or naming something that was never
    /// offered. Counted and shown, because a discarded answer is a fact about
    /// the model and not something to hide.
    pub discarded: u32,
    /// Decisions from the rule baseline. Zero while playing.
    pub from_rules: u32,
    latency_sum_ms: u64,
    pub input_tokens: u64,
}

impl DecisionStats {
    pub fn record(&mut self, origin: &Origin) {
        match origin {
            Origin::Jev {
                latency_ms,
                input_tokens,
                ..
            } => {
                self.from_jev += 1;
                self.latency_sum_ms += u64::from(*latency_ms);
                self.input_tokens += u64::from(*input_tokens);
            }
            Origin::Rules => self.from_rules += 1,
        }
    }

    pub fn average_latency_ms(&self) -> u64 {
        self.latency_sum_ms
            .checked_div(u64::from(self.from_jev))
            .unwrap_or(0)
    }

    /// Output tokens are free, so the input count is the whole bill.
    pub fn cost_usd(&self) -> f64 {
        self.input_tokens as f64 * crate::api::types::USD_PER_INPUT_TOKEN
    }

    /// What an hour of this colony would cost at the measured request size.
    /// Zero until the first answer, because guessing would defeat the purpose.
    pub fn projected_usd_per_hour(&self, ants: usize, interval_seconds: f32) -> f64 {
        if self.from_jev == 0 || interval_seconds <= 0.0 {
            return 0.0;
        }
        let tokens_per_request = self.input_tokens as f64 / f64::from(self.from_jev);
        let requests_per_hour = ants as f64 / f64::from(interval_seconds) * 3600.0;
        tokens_per_request * requests_per_hour * crate::api::types::USD_PER_INPUT_TOKEN
    }
}

#[derive(Resource)]
pub struct ActiveSource(pub Box<dyn DecisionSource>);

/// What the queen last said. The text is the player's, and it is handed to the
/// model unchanged — translating it would be a decision in itself.
#[derive(Resource, Default)]
pub struct QueenOrder(pub String);

/// Each ant thinks on its own clock. The offsets spread the requests evenly over
/// the interval instead of firing all of them in the same frame.
#[derive(Component)]
pub struct ThinkTimer(Timer);

impl ThinkTimer {
    /// Makes the ant ask on the next tick instead of waiting out the interval.
    /// Used when an intent finishes or falls apart, and when the queen speaks —
    /// event-driven questions matter more than a short fixed interval
    /// (`ANTS.md` §9).
    pub fn ask_now(&mut self) {
        let interval = self.0.duration();
        self.0.set_elapsed(interval);
    }

    pub fn staggered(index: usize, count: usize) -> Self {
        let mut timer = Timer::from_seconds(THINK_INTERVAL, TimerMode::Repeating);
        let offset = THINK_INTERVAL * index as f32 / count.max(1) as f32;
        timer.set_elapsed(Duration::from_secs_f32(offset));
        Self(timer)
    }
}

/// Asking runs before applying, every frame.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ThinkSet;

/// Where decisions come from.
///
/// No `Default` on purpose. A default would have made `DecisionsPlugin::default()`
/// in a test quietly fire real requests at the live API.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DecisionsFrom {
    /// The live model. This is the game; without a key the colony sleeps.
    Jev,
    /// Plain rules. **Not a way to play** — only the offline tests and the
    /// measurement baseline ask for this.
    RulesForTesting,
}

pub struct DecisionsPlugin {
    pub source: DecisionsFrom,
}

impl Plugin for DecisionsPlugin {
    fn build(&self, app: &mut App) {
        let source: Box<dyn DecisionSource> = match self.source {
            DecisionsFrom::Jev => match Client::from_env() {
                Some(client) => {
                    info!("decisions come from Jev");
                    Box::new(jev::JevSource::new(client))
                }
                None => {
                    // No stand-in on purpose. A colony that kept working on
                    // rules would hide what the model contributes, and this
                    // game exists to show exactly that.
                    warn!("no TYPESAFE_API_KEY — the colony will not move");
                    Box::new(Asleep)
                }
            },
            DecisionsFrom::RulesForTesting => Box::new(rules::RuleSource::default()),
        };

        app.insert_resource(ActiveSource(source))
            .init_resource::<QueenOrder>()
            .init_resource::<DecisionStats>()
            .add_systems(Update, request_decisions.in_set(ThinkSet));
    }
}

/// Every ant, with the two things that say whether it is available: a step in
/// progress, and an intent it is already pursuing.
type AntsToAsk<'w, 's> = Query<
    'w,
    's,
    (
        &'static AntId,
        &'static GridPos,
        &'static Carrying,
        &'static mut ThinkTimer,
        Option<&'static MoveAnim>,
        Option<&'static Intent>,
    ),
>;

fn request_decisions(
    time: Res<Time>,
    board: BoardRead,
    order: Res<QueenOrder>,
    mut source: ResMut<ActiveSource>,
    mut ants: AntsToAsk,
) {
    let grid = *board.grid;
    let nest = *board.nest;
    // A new order reaches every ant at once; that is the one thing worth
    // interrupting a running intent for.
    let new_order = order.is_changed();

    for (id, position, carrying, mut timer, walking, intent) in &mut ants {
        let due = timer.0.tick(time.delta()).just_finished();
        if !due && !new_order {
            continue;
        }
        if walking.is_some() {
            continue; // mid-step; it will be asked again next round
        }
        if intent.is_some() && !new_order {
            // Busy pursuing something. Asking now would throw away the answer
            // we already paid for.
            continue;
        }

        // Asked — so the interval starts over. Without this an ant asks again
        // on the very next frame.
        timer.0.reset();

        source.0.request(&AntView {
            id: *id,
            order: &order.0,
            options: options::available(
                grid,
                &board.occupancy,
                nest,
                &board.scent,
                position.0,
                carrying.fruit.is_some(),
                VISION_RADIUS,
            ),
            sightings: sightings(
                grid,
                &board.occupancy,
                nest,
                &board.scent,
                position.0,
                VISION_RADIUS,
            ),
            carrying: carrying.fruit.is_some(),
        });
    }
}

/// The board, read-only. Grouped so the system keeps a readable signature.
#[derive(bevy::ecs::system::SystemParam)]
struct BoardRead<'w> {
    grid: Res<'w, Grid>,
    nest: Res<'w, Nest>,
    occupancy: Res<'w, Occupancy>,
    scent: Res<'w, Scent>,
}

/// No key, no answers. The colony stands still, and the bar at the top says so.
///
/// This is deliberate: the game exists to show what Jev decides, so without Jev
/// nothing decides. Rules that quietly took over would hide the very thing one
/// came to look at.
#[derive(Default)]
pub struct Asleep;

impl DecisionSource for Asleep {
    fn request(&mut self, _ant: &AntView<'_>) {}

    fn poll(&mut self) -> Vec<AntMove> {
        Vec::new()
    }

    fn name(&self) -> &'static str {
        "nobody — no API key"
    }
}
