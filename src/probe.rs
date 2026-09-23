//! `cargo run -- --probe`
//!
//! One real request against the live API, printed to the console. This is the
//! point where the documentation meets reality: it confirms the endpoint path
//! and the field names, and it measures the two numbers the whole cost and rate
//! planning rests on — latency and input tokens.
//!
//! The state has the shape an ant will really send: egocentric, three cells of
//! sight, no absolute coordinates.

use std::thread::sleep;
use std::time::Duration;

use crate::ants::components::AntId;
use crate::api::types::{Answer, ApiError, Question, SystemOneRequest, USD_PER_INPUT_TOKEN};
use crate::api::{Client, ENDPOINT_PATH, Reply};
use crate::config::{ANT_COUNT, THINK_INTERVAL};
use crate::decisions::Action;
use crate::decisions::AntView;
use crate::decisions::jev::build_request;
use crate::decisions::questions::DEFAULT_STEP;
use crate::world::grid::Dir;

const GIVE_UP_AFTER: Duration = Duration::from_secs(10);

pub fn run() {
    let Some(client) = Client::from_env() else {
        eprintln!("No TYPESAFE_API_KEY found.");
        eprintln!("Put it in .env (see .env.example) or export it in the shell.");
        return;
    };

    let request = example_request();
    println!("POST {ENDPOINT_PATH}");
    println!(
        "{}\n",
        serde_json::to_string_pretty(&request).expect("the request serialises")
    );

    match ask_and_wait(&client, &request) {
        Ok(reply) => report(&reply),
        Err(error) => {
            eprintln!("Failed: {error}");
            if error.is_transient() {
                eprintln!("That one is worth retrying; the request itself looks fine.");
            }
        }
    }
}

/// Sends one request and waits for the answer. Only for the command-line tools:
/// the game itself never waits for anything.
pub fn ask_and_wait(client: &Client, request: &SystemOneRequest) -> Result<Reply, ApiError> {
    let pending = client.send(request);
    let mut waited = Duration::ZERO;
    let step = Duration::from_millis(10);

    loop {
        if let Some(result) = pending.poll() {
            return result;
        }
        if waited >= GIVE_UP_AFTER {
            return Err(ApiError::Transport(format!(
                "no answer within {GIVE_UP_AFTER:?}"
            )));
        }
        sleep(step);
        waited += step;
    }
}

/// Exactly what an ant sends — built by the same code the game uses, so the
/// measured token count is the real one. Only the extra `noul` is added here,
/// to see what a second question against the same state costs.
fn example_request() -> SystemOneRequest {
    let view = AntView {
        id: AntId(0),
        order: "Geht alle nach Osten",
        instructions: DEFAULT_STEP,
        options: [Dir::North, Dir::East, Dir::SouthEast, Dir::West]
            .into_iter()
            .map(Action::Walk)
            .chain([Action::Wait])
            .collect(),
        sightings: vec![
            "another ant, 2 cells to the north".to_string(),
            "another ant, 3 cells to the south-east".to_string(),
            "the edge of the world, 1 cell to the north-west".to_string(),
        ],
        carrying: false,
    };

    let mut request = build_request(&view);
    request.questions.insert(
        "follows_order".to_string(),
        Question::Noul {
            instructions: "Is the ant queen's order relevant to this ant right now?".to_string(),
            criteria: None,
        },
    );
    request
}

fn report(reply: &Reply) {
    let response = &reply.response;
    println!("Answered by {} in {} ms", response.model, reply.latency_ms);

    for (name, answer) in &response.answers {
        match answer {
            Answer::Choice {
                choice,
                probabilities,
                confidence,
            } => {
                println!("  {name}: {choice}  (confidence {confidence:.2})");
                // The full distribution is the actual product here — a calibrated
                // model is only worth something if you look at it.
                for (option, probability) in probabilities {
                    println!("      {probability:5.3}  {option}");
                }
            }
            Answer::Noul { noul } => println!("  {name}: {noul:.2}"),
            Answer::Unsupported => println!("  {name}: answer type not modelled yet"),
        }
    }

    let usage = &response.usage;
    println!(
        "\n{} input tokens, {} output tokens — ${:.6} for this request",
        usage.input_tokens,
        usage.output_tokens,
        usage.cost_usd()
    );

    // What the measured size means for a colony that keeps running.
    let requests_per_minute = 60.0 / THINK_INTERVAL * ANT_COUNT as f32;
    let cost_per_hour =
        f64::from(usage.input_tokens) * USD_PER_INPUT_TOKEN * f64::from(requests_per_minute) * 60.0;
    println!(
        "At {ANT_COUNT} ants every {THINK_INTERVAL:.1} s: {requests_per_minute:.0} requests/min \
         ({:.0} % of the 1200/min limit), about ${cost_per_hour:.2} per hour.",
        requests_per_minute / 1200.0 * 100.0
    );
}
