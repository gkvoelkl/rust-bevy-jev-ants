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
pub mod random;

use std::time::Duration;

use bevy::prelude::*;

use crate::ants::components::{AntId, GridPos, MoveAnim};
use crate::api::Client;
use crate::config::{THINK_INTERVAL, VISION_RADIUS};
use crate::world::grid::{Dir, Grid, Occupancy, free_directions};
use crate::world::vision::sightings;

/// What one ant knows and may do right now. This is the whole world as far as
/// the decision is concerned — no board size, no coordinates, no other ants
/// beyond what it can see.
pub struct AntView<'a> {
    pub id: AntId,
    /// The queen's order, passed through word for word. Empty means none.
    pub order: &'a str,
    /// Generated at runtime from what this ant can reach — never a hardcoded list.
    pub options: Vec<Dir>,
    /// Sentences describing what is within sight.
    pub sightings: Vec<String>,
}

/// One ant's decision, together with where it came from. The origin is what
/// makes the HUD honest: a step the classic rules took costs nothing and has no
/// confidence, and it must not be counted as if the model had decided it.
pub struct AntMove {
    pub id: AntId,
    pub dir: Dir,
    pub origin: Origin,
}

pub enum Origin {
    Jev {
        confidence: f32,
        latency_ms: u32,
        input_tokens: u32,
    },
    /// The pheromone rules. No model, no cost.
    Classic,
}

pub trait DecisionSource: Send + Sync {
    /// Asks for a single ant. Must return immediately; the answer arrives
    /// through `poll`.
    fn request(&mut self, ant: &AntView<'_>);
    /// Everything that has arrived since the last call. Empty is the normal case.
    fn poll(&mut self) -> Vec<AntMove>;
    /// For the HUD, so the player can see what is steering the colony.
    fn name(&self) -> &'static str;
}

/// What the colony has cost and how fast it thinks. Fed by every decision that
/// comes in, so the numbers are measured rather than estimated.
#[derive(Resource, Default)]
pub struct DecisionStats {
    pub from_jev: u32,
    pub from_classic: u32,
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
            Origin::Classic => self.from_classic += 1,
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

/// Which source the game runs on. `Random` never touches the network, which is
/// also what keeps the tests offline.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SourceKind {
    #[default]
    Random,
    /// Falls back to `Random` when there is no key.
    Jev,
}

#[derive(Default)]
pub struct DecisionsPlugin {
    pub source: SourceKind,
}

impl Plugin for DecisionsPlugin {
    fn build(&self, app: &mut App) {
        let source: Box<dyn DecisionSource> = match self.source {
            SourceKind::Jev => match Client::from_env() {
                Some(client) => {
                    info!("decisions come from Jev");
                    Box::new(jev::JevSource::new(client))
                }
                None => {
                    warn!("no TYPESAFE_API_KEY — the colony runs on the classic rules");
                    Box::new(random::RandomSource::default())
                }
            },
            SourceKind::Random => Box::new(random::RandomSource::default()),
        };

        app.insert_resource(ActiveSource(source))
            .init_resource::<QueenOrder>()
            .init_resource::<DecisionStats>()
            .add_systems(Update, request_decisions.in_set(ThinkSet));
    }
}

fn request_decisions(
    time: Res<Time>,
    grid: Res<Grid>,
    occupancy: Res<Occupancy>,
    order: Res<QueenOrder>,
    mut source: ResMut<ActiveSource>,
    mut ants: Query<(&AntId, &GridPos, &mut ThinkTimer, Option<&MoveAnim>)>,
) {
    let grid = *grid;
    for (id, position, mut timer, walking) in &mut ants {
        if !timer.0.tick(time.delta()).just_finished() {
            continue;
        }
        if walking.is_some() {
            continue; // still on its way; it will be asked again next round
        }
        source.0.request(&AntView {
            id: *id,
            order: &order.0,
            options: free_directions(grid, &occupancy, position.0),
            sightings: sightings(grid, &occupancy, position.0, VISION_RADIUS),
        });
    }
}
