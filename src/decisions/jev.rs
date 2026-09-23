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

use super::{Action, AntMove, AntView, DecisionSource, Exchange, Link, Origin, Outcome};

/// The question key. One question in step 1; `urgency` and `follows_order`
/// join it against the same state in step 2.
pub const STEP_QUESTION: &str = "step";

/// A request whose answer has not arrived yet.
struct InFlight {
    ant: AntId,
    /// Exactly the options this ant was offered. An answer is checked against
    /// this list, so an action that was never offered can never be carried out.
    offered: Vec<Action>,
    /// What else went out with it, kept so the inspector can show the exchange
    /// whole instead of a reconstruction of it. The ant will have moved on by
    /// the time the answer lands, and its view with it.
    asked: Asked,
    pending: Pending,
    /// Insurance against a callback that never fires; `ehttp` already times out.
    deadline: Instant,
}

/// The request side of an exchange, held while the answer is out.
struct Asked {
    order: String,
    carrying: bool,
    sightings: Vec<String>,
    instructions: String,
    /// The body that went out, formatted for reading. The wire carries it
    /// compact; the indentation is the only difference.
    body: String,
}

/// Puts the two halves together once the answer is in.
fn exchange(flight: &InFlight, outcome: Outcome, response: Option<String>) -> Exchange {
    Exchange {
        ant: flight.ant,
        order: flight.asked.order.clone(),
        carrying: flight.asked.carrying,
        sightings: flight.asked.sightings.clone(),
        instructions: flight.asked.instructions.clone(),
        request: flight.asked.body.clone(),
        response,
        offered: flight
            .offered
            .iter()
            .map(|option| (option.key(), option.description()))
            .collect(),
        outcome,
    }
}

pub struct JevSource {
    client: Client,
    in_flight: Vec<InFlight>,
    /// Answers thrown away, and questions that never went out. Reported rather
    /// than papered over — there is nothing standing in for the model.
    discarded: u32,
    /// How the last round trip went, for the lamp. Not the same as `discarded`:
    /// an answer can arrive perfectly well and still be unusable.
    link: Link,
    /// Finished exchanges waiting to be picked up by the world.
    exchanges: Vec<Exchange>,
}

impl JevSource {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            in_flight: Vec::new(),
            discarded: 0,
            link: Link::Untried,
            exchanges: Vec::new(),
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

        let request = build_request(ant);
        let body = serde_json::to_string_pretty(&request)
            .unwrap_or_else(|error| format!("could not be shown: {error}"));
        let pending = self.client.send(&request);
        self.in_flight.push(InFlight {
            ant: ant.id,
            offered: ant.options.clone(),
            asked: Asked {
                order: ant.order.to_string(),
                carrying: ant.carrying,
                sightings: ant.sightings.clone(),
                instructions: ant.instructions.to_string(),
                body,
            },
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

    fn link(&self) -> Link {
        self.link.clone()
    }

    fn drain_exchanges(&mut self) -> Vec<Exchange> {
        std::mem::take(&mut self.exchanges)
    }

    fn poll(&mut self) -> Vec<AntMove> {
        let now = Instant::now();
        let mut moves = Vec::new();
        let mut failed: Vec<(AntId, String)> = Vec::new();

        // The line is judged by the round trip alone. An answer that arrives and
        // is then refused below still came back, and the lamp must not call that
        // a broken connection.
        let mut link: Option<Link> = None;

        // Written whole, refused or not: the record is the point of the demo,
        // and an answer that was thrown away is the more instructive half of it.
        let mut records: Vec<Exchange> = Vec::new();

        self.in_flight
            .retain_mut(|flight| match flight.pending.poll() {
                Some(Ok(reply)) => {
                    link = Some(Link::Live);
                    let probabilities = distribution(&reply.response);
                    let body = Some(pretty(&reply.body));
                    match read_step(&reply.response, &flight.offered) {
                        Ok((action, confidence)) => {
                            records.push(exchange(
                                flight,
                                Outcome::Taken {
                                    chosen: action.key(),
                                    confidence,
                                    probabilities,
                                    latency_ms: reply.latency_ms,
                                    input_tokens: reply.response.usage.input_tokens,
                                },
                                body,
                            ));
                            moves.push(AntMove {
                                id: flight.ant,
                                action,
                                origin: Origin::Jev {
                                    confidence,
                                    latency_ms: reply.latency_ms,
                                    input_tokens: reply.response.usage.input_tokens,
                                },
                            });
                        }
                        Err(reason) => {
                            records.push(exchange(
                                flight,
                                Outcome::Refused {
                                    reason: reason.clone(),
                                    probabilities,
                                    latency_ms: reply.latency_ms,
                                },
                                body,
                            ));
                            failed.push((flight.ant, reason));
                        }
                    }
                    false
                }
                Some(Err(error)) => {
                    warn!("{:?}: {error}", flight.ant);
                    link = Some(Link::Down(error.to_string()));
                    records.push(exchange(
                        flight,
                        Outcome::Failed {
                            reason: error.to_string(),
                        },
                        None,
                    ));
                    failed.push((flight.ant, error.to_string()));
                    false
                }
                None => {
                    if now >= flight.deadline {
                        let reason = "no answer before the deadline".to_string();
                        link = Some(Link::Down(reason.clone()));
                        records.push(exchange(
                            flight,
                            Outcome::Failed {
                                reason: reason.clone(),
                            },
                            None,
                        ));
                        failed.push((flight.ant, reason));
                        false
                    } else {
                        true
                    }
                }
            });

        self.exchanges.append(&mut records);

        // A green from this frame beats a red from the same frame: with several
        // requests in the air, one that came back says more about the line than
        // one that has not.
        if let Some(verdict) = link
            && (verdict == Link::Live || self.in_flight.is_empty())
        {
            self.link = verdict;
        }

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
            // Whatever the text says right now — it comes from the asset, and it
            // may well have been reworded since the last request went out.
            instructions: ant.instructions.to_string(),
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

/// The body as it came, indented so it can be read.
///
/// If it will not parse it is shown exactly as it arrived — an answer the game
/// could not make sense of is the one most worth looking at unaltered.
fn pretty(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|parsed| serde_json::to_string_pretty(&parsed).ok())
        .unwrap_or_else(|| body.to_string())
}

/// The full distribution the model returned, highest first.
///
/// Read out even when the answer is about to be refused. It costs nothing — it
/// came in the same response — and it is the only place where the shape of the
/// model's uncertainty is visible rather than summed up into one number.
fn distribution(response: &SystemOneResponse) -> Vec<(String, f32)> {
    let Some(Answer::Choice { probabilities, .. }) = response.answers.get(STEP_QUESTION) else {
        return Vec::new();
    };
    let mut sorted: Vec<(String, f32)> = probabilities
        .iter()
        .map(|(key, weight)| (key.clone(), *weight))
        .collect();
    // Ties broken by key, so the same answer always lists the same way.
    sorted.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    sorted
}

/// The answer, checked twice: the option must have been offered, and the model
/// must be sure enough. Either check failing means the ant does not act this
/// round — nothing decides in its place.
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
            instructions: crate::decisions::questions::DEFAULT_STEP,
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

    /// A whole distribution, so the inspector has something with a shape.
    fn spread(pairs: &[(&str, f32)], choice: &str, confidence: f32) -> SystemOneResponse {
        let probabilities: Vec<String> = pairs
            .iter()
            .map(|(key, weight)| format!("\"{key}\":{weight}"))
            .collect();
        serde_json::from_str(&format!(
            r#"{{"model":"jev-1.13.0",
                 "answers":{{"step":{{"type":"choice","choice":"{choice}",
                              "probabilities":{{{}}},
                              "confidence":{confidence}}}}},
                 "usage":{{"input_tokens":180,"output_tokens":1}}}}"#,
            probabilities.join(",")
        ))
        .expect("parses")
    }

    /// The numbers the inspector draws. Highest first, because that is the only
    /// order in which a distribution reads as an answer.
    #[test]
    fn the_distribution_comes_back_sorted() {
        let response = spread(
            &[("east", 0.2), ("north", 0.62), ("stay", 0.18)],
            "north",
            0.5,
        );
        let sorted = distribution(&response);
        assert_eq!(
            sorted
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            ["north", "east", "stay"]
        );
    }

    /// Ties resolve by key, so the same answer never lists two different ways.
    #[test]
    fn a_tie_is_broken_the_same_way_every_time() {
        let response = spread(
            &[("west", 0.25), ("east", 0.25), ("north", 0.5)],
            "north",
            0.4,
        );
        let sorted = distribution(&response);
        assert_eq!(
            sorted
                .iter()
                .map(|(key, _)| key.as_str())
                .collect::<Vec<_>>(),
            ["north", "east", "west"]
        );
    }

    /// The case the inspector exists for: the answer is refused, and the numbers
    /// behind it survive anyway. Throwing them away with the answer would hide
    /// exactly why it was refused.
    #[test]
    fn a_refused_answer_still_has_its_distribution() {
        let response = spread(
            &[("east", 0.34), ("north", 0.33), ("west", 0.33)],
            "east",
            0.05,
        );
        assert!(read_step(&response, &[Action::Walk(Dir::East)]).is_err());
        assert_eq!(distribution(&response).len(), 3);
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
    /// 0.16 to 0.19 when no order was given at all. Such an answer is thrown
    /// away and the ant stands still for a round; it is counted in the HUD
    /// rather than covered up.
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
