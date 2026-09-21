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

use super::{Action, AntMove, AntView, DecisionSource, Origin};

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
    /// this list, so an action that was never offered can never be carried out.
    offered: Vec<Action>,
    pending: Pending,
    /// Insurance against a callback that never fires; `ehttp` already times out.
    deadline: Instant,
}

pub struct JevSource {
    client: Client,
    in_flight: Vec<InFlight>,
    /// Answers thrown away, and questions that never went out. Reported rather
    /// than papered over — there is nothing standing in for the model.
    discarded: u32,
}

impl JevSource {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            in_flight: Vec::new(),
            discarded: 0,
        }
    }

    /// Whatever went wrong, the ant simply does not act this round and is asked
    /// again when its interval comes round. Nothing decides in the model's
    /// place — that is the point of this game.
    fn discard(&mut self, ant: AntId, reason: &str) {
        self.discarded += 1;
        debug!("{ant:?}: no decision this round — {reason}");
    }
}

impl DecisionSource for JevSource {
    fn request(&mut self, ant: &AntView<'_>) {
        if self.in_flight.len() >= MAX_IN_FLIGHT {
            // Queueing is no good — by the time a queued request went out, the
            // ant would have moved on. So the question is dropped, and the ant
            // waits for its next turn. Silently dropping it once made ants
            // stand for ever; now it is counted and shown.
            self.discard(ant.id, "too many questions already in the air");
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

    fn discarded(&self) -> u32 {
        self.discarded
    }

    fn poll(&mut self) -> Vec<AntMove> {
        let now = Instant::now();
        let mut moves = Vec::new();
        let mut failed: Vec<(AntId, String)> = Vec::new();

        self.in_flight
            .retain_mut(|flight| match flight.pending.poll() {
                Some(Ok(reply)) => {
                    match read_step(&reply.response, &flight.offered) {
                        Ok((action, confidence)) => moves.push(AntMove {
                            id: flight.ant,
                            action,
                            origin: Origin::Jev {
                                confidence,
                                latency_ms: reply.latency_ms,
                                input_tokens: reply.response.usage.input_tokens,
                            },
                        }),
                        Err(reason) => {
                            failed.push((flight.ant, reason));
                        }
                    }
                    false
                }
                Some(Err(error)) => {
                    warn!("{:?}: {error}", flight.ant);
                    failed.push((flight.ant, error.to_string()));
                    false
                }
                None => {
                    if now >= flight.deadline {
                        failed.push((flight.ant, "no answer before the deadline".to_string()));
                        false
                    } else {
                        true
                    }
                }
            });

        for (ant, reason) in failed {
            self.discard(ant, &reason);
        }
        moves
    }
}

/// One ant's view, turned into a request. Public so `--probe` measures exactly
/// what the game sends, not an approximation of it.
pub fn build_request(ant: &AntView<'_>) -> SystemOneRequest {
    let criteria: BTreeMap<String, String> = ant
        .options
        .iter()
        .map(|option| (option.key(), option.description()))
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
    // Left out when the ant is empty-handed, like the empty order.
    if ant.carrying {
        state.insert("carrying".to_string(), json!("a fruit"));
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
fn read_step(response: &SystemOneResponse, offered: &[Action]) -> Result<(Action, f32), String> {
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

    // Looking the key up in the list the ant was offered is both the parse and
    // the guard: anything the model made up is simply not in there.
    let action = offered
        .iter()
        .copied()
        .find(|option| option.key() == *choice)
        .ok_or_else(|| format!("'{choice}' was never offered to this ant"))?;

    Ok((action, *confidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::grid::Dir;

    fn view<'a>(order: &'a str, options: Vec<Action>, sightings: Vec<String>) -> AntView<'a> {
        AntView {
            id: AntId(7),
            order,
            options,
            sightings,
            carrying: false,
        }
    }

    #[test]
    fn the_request_offers_only_what_the_ant_can_reach() {
        let request = build_request(&view(
            "",
            vec![Action::Walk(Dir::East), Action::Wait],
            Vec::new(),
        ));
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
            vec![Action::Walk(Dir::East)],
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
        let offered = [Action::Walk(Dir::East), Action::Wait];
        let result = read_step(&answer("east", 0.99), &offered);
        assert_eq!(result, Ok((Action::Walk(Dir::East), 0.99)));
    }

    /// The heart of the demo: an option that was not on the list cannot win,
    /// however sure the model is.
    #[test]
    fn an_option_that_was_never_offered_is_refused() {
        let offered = [Action::Walk(Dir::East), Action::Wait];
        let result = read_step(&answer("north", 0.99), &offered);
        assert!(result.is_err());
    }

    /// A flat distribution means the model has nothing to go on — measured at
    /// 0.16 to 0.19 when no order was given at all. Then the classic rules take
    /// over, which is what keeps the ants moving instead of freezing.
    #[test]
    fn an_answer_close_to_chance_is_refused() {
        let offered = [Action::Walk(Dir::East), Action::Wait];
        let result = read_step(&answer("east", 0.18), &offered);
        assert!(result.is_err());
    }

    /// The case the old threshold of 0.5 threw away: a real but nuanced
    /// preference, such as an ant moving away from its neighbours on
    /// "Verteilt euch". Those measured between 0.29 and 0.44.
    #[test]
    fn a_nuanced_but_real_preference_is_kept() {
        let offered = [Action::Walk(Dir::East), Action::Wait];
        let result = read_step(&answer("east", 0.33), &offered);
        assert_eq!(result, Ok((Action::Walk(Dir::East), 0.33)));
    }

    /// An intent is offered by its own key and read back by it.
    #[test]
    fn fetching_is_read_back_from_its_key() {
        let fetch = Action::Fetch {
            fruit: Entity::from_raw_u32(3).unwrap(),
            dir: Dir::NorthEast,
            distance: 2,
        };
        let offered = [fetch, Action::Wait];

        let result = read_step(&answer("fetch_north_east", 0.90), &offered);
        assert_eq!(result, Ok((fetch, 0.90)));
    }

    /// Carrying home when the ant holds nothing was never offered, so it cannot
    /// happen — whatever the model says.
    #[test]
    fn carrying_home_empty_handed_is_refused() {
        let offered = [Action::Walk(Dir::East), Action::Wait];
        let result = read_step(&answer("carry_home", 0.99), &offered);
        assert!(result.is_err());
    }

    #[test]
    fn a_direction_that_does_not_exist_is_refused() {
        let result = read_step(&answer("upwards", 0.99), &[Action::Walk(Dir::East)]);
        assert!(result.is_err());
    }
}
