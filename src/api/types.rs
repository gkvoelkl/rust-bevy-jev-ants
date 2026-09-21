//! The wire format of `POST /v1/systemone`.
//!
//! Field names follow the API reference at <https://docs.typesafe.ai/api>.
//! Only what the game actually sends and reads is modelled — `score` answers
//! arrive with step 2, when `urgency` is asked for the first time.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// The alias that always points at the current model.
pub const MODEL: &str = "jev-latest";

/// Input tokens are billed at $0.042 per million; output is free.
pub const USD_PER_INPUT_TOKEN: f64 = 0.042 / 1_000_000.0;

#[derive(Serialize)]
pub struct SystemOneRequest {
    pub model: &'static str,
    /// Anything the model should judge. For an ant this is what it can see.
    pub state: serde_json::Value,
    /// Questions are evaluated independently and in parallel, all against the
    /// same state. Ordered, so a request is reproducible byte for byte.
    pub questions: BTreeMap<String, Question>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Question {
    /// Pick one of up to 255 named options.
    Choice {
        instructions: String,
        /// Option key -> description. Built at runtime, never hardcoded.
        criteria: BTreeMap<String, String>,
    },
    /// Yes/no with a probability.
    Noul {
        instructions: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        criteria: Option<String>,
    },
}

#[derive(Debug, Deserialize)]
pub struct SystemOneResponse {
    /// The concrete version behind the alias, e.g. `jev-1.13.0`.
    pub model: String,
    pub answers: BTreeMap<String, Answer>,
    pub usage: Usage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Choice {
        /// The option with the highest probability.
        choice: String,
        /// The full distribution; sums to 1.
        probabilities: BTreeMap<String, f32>,
        /// How concentrated the distribution is, 0 to 1.
        confidence: f32,
    },
    Noul {
        noul: f32,
    },
    /// Anything not modelled yet, so an unexpected answer type cannot make the
    /// whole response fail to parse.
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl Usage {
    pub fn cost_usd(&self) -> f64 {
        f64::from(self.input_tokens) * USD_PER_INPUT_TOKEN
    }
}

#[derive(Debug)]
pub enum ApiError {
    /// Never reached the server: no network, DNS, CORS, timeout.
    Transport(String),
    /// 401 — the key is wrong or gone.
    Unauthorized,
    /// 422 — we built a request the API rejects. A bug on our side.
    Invalid(String),
    /// 429 — too many requests. Back off.
    RateLimited,
    /// 529 — the service is overloaded. Back off.
    Overloaded,
    Status(u16, String),
    /// The answer arrived but did not look the way the docs describe.
    Decode(String),
}

impl ApiError {
    pub fn from_status(status: u16, body: String) -> Self {
        match status {
            401 => ApiError::Unauthorized,
            422 => ApiError::Invalid(body),
            429 => ApiError::RateLimited,
            529 => ApiError::Overloaded,
            other => ApiError::Status(other, body),
        }
    }

    /// Whether waiting and asking again is worth it.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ApiError::Transport(_) | ApiError::RateLimited | ApiError::Overloaded
        )
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Transport(detail) => write!(f, "the request never got through: {detail}"),
            ApiError::Unauthorized => write!(f, "401 — TYPESAFE_API_KEY is missing or invalid"),
            ApiError::Invalid(detail) => write!(f, "422 — the API rejected the request: {detail}"),
            ApiError::RateLimited => write!(f, "429 — rate limited (1200 requests per minute)"),
            ApiError::Overloaded => write!(f, "529 — the service is overloaded"),
            ApiError::Status(status, detail) => write!(f, "HTTP {status}: {detail}"),
            ApiError::Decode(detail) => write!(f, "could not read the answer: {detail}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn example_request() -> SystemOneRequest {
        let mut criteria = BTreeMap::new();
        criteria.insert("east".to_string(), "Step one cell east".to_string());
        criteria.insert("stay".to_string(), "Do not move this turn".to_string());

        let mut questions = BTreeMap::new();
        questions.insert(
            "step".to_string(),
            Question::Choice {
                instructions: "Which single step should this ant take now?".to_string(),
                criteria,
            },
        );
        questions.insert(
            "follows_order".to_string(),
            Question::Noul {
                instructions: "Is the order relevant to this ant?".to_string(),
                criteria: None,
            },
        );

        SystemOneRequest {
            model: MODEL,
            state: json!({ "nearby": ["another ant, 2 cells to the north"] }),
            questions,
        }
    }

    /// Guards the field names against <https://docs.typesafe.ai/api>. If the API
    /// ever renames something, this fails before the game does.
    #[test]
    fn request_matches_the_documented_shape() {
        let sent = serde_json::to_value(example_request()).expect("serialises");
        assert_eq!(
            sent,
            json!({
                "model": "jev-latest",
                "state": { "nearby": ["another ant, 2 cells to the north"] },
                "questions": {
                    "step": {
                        "type": "choice",
                        "instructions": "Which single step should this ant take now?",
                        "criteria": {
                            "east": "Step one cell east",
                            "stay": "Do not move this turn"
                        }
                    },
                    "follows_order": {
                        "type": "noul",
                        "instructions": "Is the order relevant to this ant?"
                    }
                }
            })
        );
    }

    /// The quickstart's own example, verbatim.
    #[test]
    fn parses_the_documented_response() {
        let response: SystemOneResponse = serde_json::from_str(
            r#"{
                "model": "jev-1.13.0",
                "answers": { "urgency": { "type": "noul", "noul": 1.0 } },
                "usage": { "input_tokens": 392, "output_tokens": 65 }
            }"#,
        )
        .expect("parses");

        assert_eq!(response.model, "jev-1.13.0");
        assert_eq!(response.usage.input_tokens, 392);
        match response.answers.get("urgency") {
            Some(Answer::Noul { noul }) => assert_eq!(*noul, 1.0),
            other => panic!("expected a noul, got {other:?}"),
        }
    }

    #[test]
    fn parses_a_choice_answer_with_its_distribution() {
        let response: SystemOneResponse = serde_json::from_str(
            r#"{
                "model": "jev-1.13.0",
                "answers": {
                    "step": {
                        "type": "choice",
                        "choice": "east",
                        "probabilities": { "east": 0.82, "stay": 0.18 },
                        "confidence": 0.82
                    }
                },
                "usage": { "input_tokens": 500, "output_tokens": 12 }
            }"#,
        )
        .expect("parses");

        match response.answers.get("step") {
            Some(Answer::Choice {
                choice,
                probabilities,
                confidence,
            }) => {
                assert_eq!(choice, "east");
                assert_eq!(probabilities.len(), 2);
                assert!((confidence - 0.82).abs() < f32::EPSILON);
            }
            other => panic!("expected a choice, got {other:?}"),
        }
    }

    /// An answer type we have not modelled yet must not take the whole response
    /// down with it — `score` arrives in step 2.
    #[test]
    fn an_unknown_answer_type_is_tolerated() {
        let response: SystemOneResponse = serde_json::from_str(
            r#"{
                "model": "jev-1.13.0",
                "answers": { "urgency": { "type": "score", "score": "calm" } },
                "usage": { "input_tokens": 1, "output_tokens": 1 }
            }"#,
        )
        .expect("parses");

        assert!(matches!(
            response.answers.get("urgency"),
            Some(Answer::Unsupported)
        ));
    }

    #[test]
    fn status_codes_become_the_documented_errors() {
        assert!(matches!(
            ApiError::from_status(401, String::new()),
            ApiError::Unauthorized
        ));
        assert!(matches!(
            ApiError::from_status(422, "bad".into()),
            ApiError::Invalid(_)
        ));
        assert!(ApiError::from_status(429, String::new()).is_transient());
        assert!(ApiError::from_status(529, String::new()).is_transient());
        // A 422 is our own bug; retrying would just repeat it.
        assert!(!ApiError::from_status(422, String::new()).is_transient());
    }

    #[test]
    fn a_million_input_tokens_cost_four_cents() {
        let usage = Usage {
            input_tokens: 1_000_000,
            output_tokens: 999,
        };
        assert!((usage.cost_usd() - 0.042).abs() < 1e-9);
    }
}
