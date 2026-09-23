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
pub mod questions;
pub mod rules;

use std::collections::VecDeque;
use std::time::Duration;

use bevy::prelude::*;

use crate::ants::components::{AntId, Carrying, LastExchange, MoveAnim};
use crate::ants::intent::Intent;
use crate::api::Client;
use crate::config::{REMEMBERED_DISCARDS, THINK_INTERVAL, VISION_RADIUS};
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
    /// Go to the plank and pick it up. Offered only when one is actually in
    /// sight and still lying on the ground.
    TakePlank {
        plank: Entity,
        dir: Dir,
        distance: i32,
    },
    /// Let go of the plank. Offered only to an ant already holding one — the
    /// way out of a wait that will never end because no second ant came.
    LetGoPlank,
    /// Follow the trail uphill, which is the way back to the nest. Only offered
    /// when there really is a trail next door, and only to a carrier — an ant
    /// with empty hands has no use for the way home.
    FollowScent(Dir),
}

impl Action {
    /// The key this action gets in a `choice` question, and what the answer is
    /// read back through. At most one fruit per direction is ever offered, and
    /// at most one trail at all, so these stay unique.
    pub fn key(self) -> String {
        match self {
            Action::Walk(direction) => direction.key().to_string(),
            Action::Wait => "stay".to_string(),
            Action::TakePlank { .. } => "take_the_plank".to_string(),
            Action::LetGoPlank => "let_go_of_the_plank".to_string(),
            Action::Fetch { dir, .. } => format!("fetch_{}", dir.key()),
            Action::CarryHome => "carry_home".to_string(),
            // No direction in the key, though the option carries one. Only one
            // trail is ever offered, so it is not needed to stay unique — and
            // `follow_scent_north` sitting next to plain `north` in the same
            // criteria list invited the model to read them as two flavours of
            // one move and split the probability between them. A split
            // distribution is a low `confidence`, and a low confidence is a
            // thrown-away answer.
            Action::FollowScent(_) => "follow_scent".to_string(),
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
            Action::TakePlank { dir, distance, .. } => {
                let cells = if distance == 1 { "cell" } else { "cells" };
                format!(
                    "Go to the plank {distance} {cells} to the {} and pick it up. Carry \
                     it to the water and lay it across, so the colony has a way over.",
                    dir.spoken()
                )
            }
            Action::LetGoPlank => "Let go of the plank and do something else".to_string(),
            // Leads on the difference from a plain compass step, not on the
            // direction: this is the option that keeps to the trail while a
            // `Walk` would leave it at the first bend.
            Action::FollowScent(direction) => format!(
                "Stay on the scent trail, starting {}, and keep to it as it bends rather \
                 than walking in a straight line. It grows stronger towards the nest, so \
                 it leads home.",
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
            Action::TakePlank { .. } => "plank".to_string(),
            Action::LetGoPlank => "let go".to_string(),
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
    /// The question being put to the model, as it stands at this moment. Not
    /// part of what the ant knows — part of what it is being asked — but it
    /// travels with the view because the text can change between two requests
    /// (`questions::Questions`).
    pub instructions: &'a str,
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

/// One complete exchange with the model for one ant: what was asked, what came
/// back, and what became of it.
///
/// This is the evidence the demo rests on, and all of it was paid for anyway.
/// The distribution in particular arrives with every single answer and used to
/// be dropped on the floor — it is the clearest look at what Jev was weighing
/// that anyone gets. The same row is what `ANTS.md` §6.2 wants written out as
/// one JSONL line, so building it once serves the inspector and the replay.
#[derive(Clone)]
pub struct Exchange {
    pub ant: AntId,
    /// The queen's order as it went out. Empty means it was left out entirely.
    pub order: String,
    pub carrying: bool,
    /// The sentences that went out as `nearby` — this ant's whole world.
    pub sightings: Vec<String>,
    /// The question text in force when the request left.
    pub instructions: String,
    /// The request body as it went out, and the response body as it came back,
    /// both as JSON. Not a reconstruction: the response is the bytes off the
    /// wire. `None` means nothing came back at all.
    pub request: String,
    pub response: Option<String>,
    /// Option key and description, exactly as offered. Built from the board at
    /// that moment, never from a list in the code: this is the part worth
    /// looking at, because what is not in here cannot be chosen.
    pub offered: Vec<(String, String)>,
    pub outcome: Outcome,
}

impl Exchange {
    pub fn was_used(&self) -> bool {
        matches!(self.outcome, Outcome::Taken { .. })
    }

    /// The distribution, if one ever arrived. Highest first.
    pub fn probabilities(&self) -> &[(String, f32)] {
        match &self.outcome {
            Outcome::Taken { probabilities, .. } | Outcome::Refused { probabilities, .. } => {
                probabilities
            }
            Outcome::Failed { .. } => &[],
        }
    }
}

/// What became of one answer.
#[derive(Clone)]
pub enum Outcome {
    /// It moved the ant.
    Taken {
        chosen: String,
        confidence: f32,
        probabilities: Vec<(String, f32)>,
        latency_ms: u32,
        input_tokens: u32,
    },
    /// An answer came back and was refused. The distribution is kept all the
    /// same — a refused answer teaches more than an accepted one, because the
    /// reason is visible in the numbers.
    Refused {
        reason: String,
        probabilities: Vec<(String, f32)>,
        latency_ms: u32,
    },
    /// Nothing came back: no key, no network, no answer in time.
    Failed { reason: String },
}

/// The last few answers that did not become a move, newest first.
///
/// A counter says that something went wrong; this says what. The two reasons
/// are the whole lesson — an answer too close to chance, and an answer naming
/// something the ant was never offered. The second is the type safety catching
/// an invented target in the act, which is the one thing this game exists to
/// show, and it used to scroll past in a debug log.
#[derive(Resource, Default)]
pub struct DiscardLog(pub VecDeque<Exchange>);

impl DiscardLog {
    pub fn remember(&mut self, exchange: Exchange) {
        self.0.push_front(exchange);
        self.0.truncate(REMEMBERED_DISCARDS);
    }
}

/// What the last exchange with the decision source looked like — no more than
/// the lamp in the corner needs.
///
/// Deliberately about the **round trip**, not about whether the answer was
/// usable. An answer that comes back and is then thrown away for being too
/// close to chance still proves the line is up; that it was discarded is a
/// different fact, and the counter next to the lamp carries it. Keeping the two
/// apart is half of what there is to learn here.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Link {
    /// Nothing has been tried yet. Grey, not red: an untried line is not a
    /// broken one, and until the queen speaks nothing is tried at all.
    Untried,
    /// An answer came back. Green.
    Live,
    /// Unusable, and why. A missing key counts the same as a dead network —
    /// from where the player sits, both mean no answers. Red.
    Down(String),
}

impl Link {
    /// The words next to the lamp. Short, because the detail is one hover away.
    pub fn short(&self) -> &'static str {
        match self {
            Link::Untried => "Jev — not asked yet",
            Link::Live => "Jev — answering",
            Link::Down(_) => "Jev — not answering",
        }
    }

    /// The whole story, for the tooltip.
    pub fn detail(&self) -> String {
        match self {
            Link::Untried => {
                "No request has gone out yet. The colony sleeps until the queen's first \
                 order, and nothing is asked — and nothing paid for — before that."
                    .to_string()
            }
            Link::Live => "The last request came back. Whether its answer was used is a separate \
                 question — see the discarded count."
                .to_string(),
            Link::Down(reason) => format!("The last attempt failed: {reason}"),
        }
    }
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
    /// For the lamp. The default suits any source that never touches a network:
    /// the rule baseline has no line to Jev, so it has none to report on.
    fn link(&self) -> Link {
        Link::Untried
    }
    /// Everything that came back since the last call, used or not, for the
    /// inspector and the discard log. Separate from `poll` on purpose: `poll`
    /// returns what moves ants, this returns what there is to learn from, and a
    /// refused answer belongs only in the second.
    fn drain_exchanges(&mut self) -> Vec<Exchange> {
        Vec::new()
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
    /// Requests since the level being played started.
    ///
    /// The **only** number here that starts again, and everything else is
    /// deliberately left alone. A new level is a new attempt, not a new wallet:
    /// the tokens are already spent, and a counter that forgot them every time
    /// the player tried another board would understate the bill by however many
    /// boards they tried. What does belong to the attempt is "requests per
    /// fruit", and that is what this field is for.
    pub this_level: u32,
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
                self.this_level += 1;
                self.latency_sum_ms += u64::from(*latency_ms);
                self.input_tokens += u64::from(*input_tokens);
            }
            Origin::Rules => {
                self.from_rules += 1;
                self.this_level += 1;
            }
        }
    }

    /// A level was picked. Only the per-attempt counter goes back to nothing.
    pub fn start_new_level(&mut self) {
        self.this_level = 0;
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

/// Whether the queen has spoken at all yet.
///
/// Until she has, not one request goes out. The colony sits in the nest and
/// does nothing, because that is the honest opening: an ant knows what it can
/// see, not what it is for. Asking Jev before the queen has said anything means
/// asking "which way?" with no reason to prefer any — the answers were near
/// chance, and the ants scattered as if they had a plan.
///
/// Once awake the colony stays awake. "Say nothing" takes the order back; it
/// does not send the colony back to bed.
#[derive(Resource, Default)]
pub struct ColonyAwake(pub bool);

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

/// And writing down what came back runs after both, so the inspector shows this
/// frame's answer rather than trailing it by one.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct RecordSet;

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
            .insert_resource(questions::Questions::load_or_default())
            .init_resource::<QueenOrder>()
            .init_resource::<ColonyAwake>()
            .init_resource::<DecisionStats>()
            .init_resource::<DiscardLog>()
            .add_systems(
                Update,
                (
                    request_decisions.in_set(ThinkSet),
                    record_exchanges.in_set(RecordSet),
                ),
            );
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
    questions: Res<questions::Questions>,
    mut awake: ResMut<ColonyAwake>,
    mut source: ResMut<ActiveSource>,
    mut ants: AntsToAsk,
) {
    // Before the first order nothing is asked — see `ColonyAwake`. The think
    // timers are not even ticked: a sleeping colony has no clock, so the
    // stagger is still intact when the queen finally speaks.
    let just_woke = !awake.0 && !order.0.trim().is_empty();
    if just_woke {
        info!("the queen has spoken — the colony wakes up");
        awake.0 = true;
    }
    if !awake.0 {
        return;
    }

    let grid = *board.grid;
    let nest = *board.nest;
    // A new order reaches every ant at once; that is the one thing worth
    // interrupting a running intent for. Waking counts as one, and so does a
    // reworded question — the point of being able to edit it is to see the
    // next answers change, not the ones after that.
    let ask_everyone = order.is_changed() || questions.is_changed() || just_woke;

    for (id, position, carrying, mut timer, walking, intent) in &mut ants {
        let due = timer.0.tick(time.delta()).just_finished();
        if !due && !ask_everyone {
            continue;
        }
        if walking.is_some() {
            continue; // mid-step; it will be asked again next round
        }
        if intent.is_some() && !ask_everyone {
            // Busy pursuing something. Asking now would throw away the answer
            // we already paid for.
            continue;
        }

        // Asked — so the interval starts over. Without this an ant asks again
        // on the very next frame.
        timer.0.reset();

        // One thing at a time: a plank takes both hands, so an ant holding one
        // is in a different situation from one holding fruit or nothing.
        let hands = if matches!(intent, Some(Intent::HoldPlank { .. })) {
            options::Hands::Plank
        } else if carrying.fruit.is_some() {
            options::Hands::Fruit
        } else {
            options::Hands::Empty
        };

        source.0.request(&AntView {
            id: *id,
            order: &order.0,
            instructions: &questions.step,
            options: options::available(
                grid,
                &board.occupancy,
                nest,
                &board.scent,
                position.0,
                hands,
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

/// Takes what came back and puts it where it can be looked at: on the ant it
/// belongs to, and — if it never became a move — in the discard log.
///
/// Nothing here steers anything. It exists so that the request an ant sent and
/// the numbers it got back survive long enough to be read, which is the whole
/// difference between a colony that looks clever and one you can learn from.
fn record_exchanges(
    mut source: ResMut<ActiveSource>,
    mut log: ResMut<DiscardLog>,
    mut ants: Query<(&AntId, &mut LastExchange)>,
) {
    let fresh = source.0.drain_exchanges();
    if fresh.is_empty() {
        return;
    }

    for record in fresh {
        if let Some((_, mut last)) = ants.iter_mut().find(|(id, _)| **id == record.ant) {
            last.0 = Some(record.clone());
        }
        // An ant that has since despawned still leaves its lesson behind.
        if !record.was_used() {
            log.remember(record);
        }
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

    fn link(&self) -> Link {
        Link::Down("no TYPESAFE_API_KEY — put one in .env and restart".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The most common red lamp by far, and the one that has to explain itself:
    /// a player with no key sees a colony that never moves.
    #[test]
    fn without_a_key_the_lamp_is_red_and_says_why() {
        let link = Asleep.link();
        assert!(matches!(link, Link::Down(_)));
        assert!(
            link.detail().contains("TYPESAFE_API_KEY"),
            "the detail must name what is missing, got: {}",
            link.detail()
        );
    }

    /// A source that never touches a network has nothing to report about one.
    /// Grey is the honest answer there, not green.
    #[test]
    fn a_source_without_a_line_reports_untried() {
        assert_eq!(rules::RuleSource::default().link(), Link::Untried);
    }

    fn thrown_away(ant: u32) -> Exchange {
        Exchange {
            ant: AntId(ant),
            order: String::new(),
            carrying: false,
            sightings: Vec::new(),
            instructions: questions::DEFAULT_STEP.to_string(),
            request: "{}".to_string(),
            response: Some("{}".to_string()),
            offered: Vec::new(),
            outcome: Outcome::Refused {
                reason: "0.18 is too close to chance".to_string(),
                probabilities: Vec::new(),
                latency_ms: 700,
            },
        }
    }

    /// Newest first and bounded: the log is there to be read while the colony
    /// runs, not to grow into a second copy of the session.
    #[test]
    fn the_discard_log_keeps_the_newest_few() {
        let mut log = DiscardLog::default();
        for ant in 0..(REMEMBERED_DISCARDS as u32 + 4) {
            log.remember(thrown_away(ant));
        }

        assert_eq!(log.0.len(), REMEMBERED_DISCARDS);
        assert_eq!(
            log.0.front().expect("not empty").ant,
            AntId(REMEMBERED_DISCARDS as u32 + 3),
            "the newest one is at the front"
        );
    }

    /// A refused answer is not a decision, and must not be counted as one.
    #[test]
    fn a_refused_exchange_is_not_a_used_one() {
        assert!(!thrown_away(0).was_used());
    }
}
