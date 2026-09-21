//! The live source: one request per ant, answered whenever it is answered.
//!
//! This is the only module that knows both an ant and the API. It translates a
//! single ant's view into a state, offers exactly the moves that ant can make,
//! and checks the answer against that list again when it arrives.

use std::collections::BTreeMap;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use serde_json::{Value, json};

use crate::ants::components::AntId;
use crate::api::types::{Answer, MODEL, Question, SystemOneRequest, SystemOneResponse};
use crate::api::{Client, Pending, REQUEST_TIMEOUT};
use crate::config::{MAX_IN_FLIGHT, MIN_CONFIDENCE, VISION_RADIUS};
use crate::world::grid::Dir;

use super::random::RandomSource;
use super::{AntMove, AntView, DecisionSource, Origin};

/// The question key. One question in step 1; `urgency` and `follows_order`
/// join it against the same state in step 2.
pub const STEP_QUESTION: &str = "step";

/// Moved to `assets/questions.ron` later — rewording this text without
/// recompiling is the actual development effort.
const STEP_INSTRUCTIONS: &str = "Which single step should this ant take now? \
     Follow the ant queen's order whenever it applies.";

/// A request whose answer has not arrived yet.
struct InFlight {
    ant: AntId,
    /// Exactly the options this ant was offered. An answer is checked against
    /// this list, so a direction that was never offered can never become a move.
    offered: Vec<Dir>,
    pending: Pending,
    /// Insurance against a callback that never fires; `ehttp` already times out.
    deadline: Instant,
}

pub struct JevSource {
    client: Client,
    in_flight: Vec<InFlight>,
    /// What a single ant falls back to when the model cannot answer for it.
    classic: RandomSource,
}

impl JevSource {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            in_flight: Vec::new(),
            classic: RandomSource::default(),
        }
    }

    fn fall_back(&mut self, ant: AntId, offered: Vec<Dir>) {
        self.classic.request(&AntView {
            id: ant,
            order: "",
            options: offered,
            sightings: Vec::new(),
        });
    }
}

impl DecisionSource for JevSource {
    fn request(&mut self, ant: &AntView<'_>) {
        if self.in_flight.len() >= MAX_IN_FLIGHT {
            // Skipping beats queueing: by the time a queued request went out,
            // the ant would have moved on and the answer would be stale.
            return;
        }

        let pending = self.client.send(&build_request(ant));
        self.in_flight.push(InFlight {
            ant: ant.id,
            offered: ant.options.clone(),
            pending,
            deadline: Instant::now() + REQUEST_TIMEOUT * 2,
        });
    }

    fn name(&self) -> &'static str {
        "Jev"
    }

    fn poll(&mut self) -> Vec<AntMove> {
        let now = Instant::now();
        let mut moves = Vec::new();
        let mut failed: Vec<(AntId, Vec<Dir>)> = Vec::new();

        self.in_flight
            .retain_mut(|flight| match flight.pending.poll() {
                Some(Ok(reply)) => {
                    match read_step(&reply.response, &flight.offered) {
                        Ok((direction, confidence)) => moves.push(AntMove {
                            id: flight.ant,
                            dir: direction,
                            origin: Origin::Jev {
                                confidence,
                                latency_ms: reply.latency_ms,
                                input_tokens: reply.response.usage.input_tokens,
                            },
                        }),
                        Err(reason) => {
                            debug!("{:?} falls back to the classic rules: {reason}", flight.ant);
                            failed.push((flight.ant, flight.offered.clone()));
                        }
                    }
                    false
                }
                Some(Err(error)) => {
                    warn!("{:?}: {error}", flight.ant);
                    failed.push((flight.ant, flight.offered.clone()));
                    false
                }
                None => {
                    if now >= flight.deadline {
                        warn!("{:?}: no answer before the deadline", flight.ant);
                        failed.push((flight.ant, flight.offered.clone()));
                        false
                    } else {
                        true
                    }
                }
            });

        for (ant, offered) in failed {
            self.fall_back(ant, offered);
        }
        moves.extend(self.classic.poll());
        moves
    }
}

/// One ant's view, turned into a request. Public so `--probe` measures exactly
/// what the game sends, not an approximation of it.
pub fn build_request(ant: &AntView<'_>) -> SystemOneRequest {
    let criteria: BTreeMap<String, String> = ant
        .options
        .iter()
        .map(|option| (option.key().to_string(), option.description()))
        .collect();

    let mut questions = BTreeMap::new();
    questions.insert(
        STEP_QUESTION.to_string(),
        Question::Choice {
            instructions: STEP_INSTRUCTIONS.to_string(),
            criteria,
        },
    );

    let mut state = serde_json::Map::new();
    // An empty order is left out entirely rather than sent as an empty string.
    if !ant.order.trim().is_empty() {
        state.insert(
            "order_from_the_ant_queen".to_string(),
            json!(ant.order.trim()),
        );
    }
    state.insert(
        "vision".to_string(),
        json!(format!(
            "This ant sees {VISION_RADIUS} cells in every direction."
        )),
    );
    state.insert(
        "nearby".to_string(),
        if ant.sightings.is_empty() {
            json!(["nothing within sight"])
        } else {
            json!(ant.sightings)
        },
    );

    SystemOneRequest {
        model: MODEL,
        state: Value::Object(state),
        questions,
    }
}

/// The answer, checked twice: the option must have been offered, and the model
/// must be sure enough. Otherwise the ant uses the classic rules.
fn read_step(response: &SystemOneResponse, offered: &[Dir]) -> Result<(Dir, f32), String> {
    let Some(Answer::Choice {
        choice, confidence, ..
    }) = response.answers.get(STEP_QUESTION)
    else {
        return Err(format!("no choice answer under '{STEP_QUESTION}'"));
    };

    // `confidence` is the distance from chance, normalised by the number of
    // options, so a cornered ant with four ways out is held to the same standard
    // as one in the open with nine.
    if *confidence < MIN_CONFIDENCE {
        return Err(format!(
            "confidence {confidence:.2} is too close to chance (below {MIN_CONFIDENCE})"
        ));
    }

    let direction = Dir::from_key(choice).ok_or_else(|| format!("unknown option '{choice}'"))?;

    if !offered.contains(&direction) {
        return Err(format!("'{choice}' was never offered to this ant"));
    }

    Ok((direction, *confidence))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view<'a>(order: &'a str, options: Vec<Dir>, sightings: Vec<String>) -> AntView<'a> {
        AntView {
            id: AntId(7),
            order,
            options,
            sightings,
        }
    }

    #[test]
    fn the_request_offers_only_what_the_ant_can_reach() {
        let request = build_request(&view("", vec![Dir::East, Dir::Stay], Vec::new()));
        let sent = serde_json::to_value(&request).expect("serialises");

        let criteria = &sent["questions"][STEP_QUESTION]["criteria"];
        assert!(criteria.get("east").is_some());
        assert!(criteria.get("stay").is_some());
        assert!(criteria.get("north").is_none(), "north was blocked");
    }

    #[test]
    fn the_state_never_carries_coordinates_or_an_empty_order() {
        let request = build_request(&view(
            "   ",
            vec![Dir::East],
            vec!["another ant, 1 cell to the north".to_string()],
        ));
        let sent = serde_json::to_value(&request).expect("serialises");

        assert!(sent["state"].get("order_from_the_ant_queen").is_none());
        let text = sent["state"].to_string();
        assert!(!text.contains("\"x\""), "no absolute coordinates: {text}");
        assert!(!text.contains("\"y\""), "no absolute coordinates: {text}");
    }

    fn answer(choice: &str, confidence: f32) -> SystemOneResponse {
        serde_json::from_str(&format!(
            r#"{{"model":"jev-1.13.0",
                 "answers":{{"step":{{"type":"choice","choice":"{choice}",
                              "probabilities":{{"{choice}":{confidence}}},
                              "confidence":{confidence}}}}},
                 "usage":{{"input_tokens":1,"output_tokens":1}}}}"#
        ))
        .expect("parses")
    }

    #[test]
    fn a_confident_offered_answer_becomes_a_move() {
        let result = read_step(&answer("east", 0.99), &[Dir::East, Dir::Stay]);
        assert_eq!(result, Ok((Dir::East, 0.99)));
    }

    /// The heart of the demo: an option that was not on the list cannot win,
    /// however sure the model is.
    #[test]
    fn an_option_that_was_never_offered_is_refused() {
        let result = read_step(&answer("north", 0.99), &[Dir::East, Dir::Stay]);
        assert!(result.is_err());
    }

    /// A flat distribution means the model has nothing to go on — measured at
    /// 0.16 to 0.19 when no order was given at all. Then the classic rules take
    /// over, which is what keeps the ants moving instead of freezing.
    #[test]
    fn an_answer_close_to_chance_is_refused() {
        let result = read_step(&answer("east", 0.18), &[Dir::East, Dir::Stay]);
        assert!(result.is_err());
    }

    /// The case the old threshold of 0.5 threw away: a real but nuanced
    /// preference, such as an ant moving away from its neighbours on
    /// "Verteilt euch". Those measured between 0.29 and 0.44.
    #[test]
    fn a_nuanced_but_real_preference_is_kept() {
        let result = read_step(&answer("east", 0.33), &[Dir::East, Dir::Stay]);
        assert_eq!(result, Ok((Dir::East, 0.33)));
    }

    #[test]
    fn a_direction_that_does_not_exist_is_refused() {
        let result = read_step(&answer("upwards", 0.99), &[Dir::East]);
        assert!(result.is_err());
    }
}
