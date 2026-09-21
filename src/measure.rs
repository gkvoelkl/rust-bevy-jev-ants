//! `cargo run -- --measure`
//!
//! The open questions from `internals/konzept.md` §10, answered with numbers
//! instead of opinions. Every request here is built by the same `build_request`
//! the game uses, so what is measured is what the colony really sends.
//!
//! One run costs a fraction of a cent.

use crate::ants::components::AntId;
use crate::api::Client;
use crate::api::types::{Answer, Question, SystemOneRequest};
use crate::decisions::AntView;
use crate::decisions::jev::{STEP_QUESTION, build_request};
use crate::probe::ask_and_wait;
use crate::world::grid::Dir;

/// The situation every experiment shares: a crowd to the north, open ground to
/// the south, a wall to the west. An order that takes the neighbours into
/// account must answer differently from one that ignores them.
fn crowded_view(order: &str) -> AntView<'_> {
    AntView {
        id: AntId(0),
        order,
        options: Dir::ALL.to_vec(),
        sightings: vec![
            "another ant, 1 cell to the north".to_string(),
            "another ant, 2 cells to the north-east".to_string(),
            "another ant, 2 cells to the north-west".to_string(),
            "the edge of the world, 2 cells to the west".to_string(),
        ],
    }
}

fn blind_view(order: &str) -> AntView<'_> {
    AntView {
        id: AntId(0),
        order,
        options: Dir::ALL.to_vec(),
        sightings: Vec::new(),
    }
}

/// The chosen option and its confidence, or a reason it could not be read.
fn ask(client: &Client, request: &SystemOneRequest) -> Result<(String, f32, u32, u32), String> {
    let reply = ask_and_wait(client, request).map_err(|error| error.to_string())?;
    match reply.response.answers.get(STEP_QUESTION) {
        Some(Answer::Choice {
            choice, confidence, ..
        }) => Ok((
            choice.clone(),
            *confidence,
            reply.response.usage.input_tokens,
            reply.latency_ms,
        )),
        other => Err(format!("unexpected answer: {other:?}")),
    }
}

pub fn run() {
    let Some(client) = Client::from_env() else {
        eprintln!("No TYPESAFE_API_KEY found. Put it in .env (see .env.example).");
        return;
    };

    questions_per_request(&client);
    order_language(&client);
    what_sight_contributes(&client);
    where_the_threshold_belongs(&client);
}

/// How many questions fit into one request, and what each extra one costs.
/// Step 2 needs the answer: `urgency` and `follows_order` join `step` there.
fn questions_per_request(client: &Client) {
    println!("\n== How many questions does one request take? ==");
    println!(
        "{:>9}  {:>7}  {:>9}  {:>9}  result",
        "questions", "tokens", "per extra", "latency"
    );

    let mut previous: Option<(usize, u32)> = None;

    for count in [1usize, 2, 4, 8, 16, 32, 64, 128] {
        let mut request = build_request(&crowded_view("Geht alle nach Osten"));

        // The first question keeps its name, so the answer stays readable.
        let template = match request.questions.get(STEP_QUESTION) {
            Some(Question::Choice {
                instructions,
                criteria,
            }) => (instructions.clone(), criteria.clone()),
            _ => unreachable!("build_request always writes one choice question"),
        };
        for index in 1..count {
            request.questions.insert(
                format!("{STEP_QUESTION}_{index:03}"),
                Question::Choice {
                    instructions: template.0.clone(),
                    criteria: template.1.clone(),
                },
            );
        }

        match ask(client, &request) {
            Ok((_, _, tokens, latency)) => {
                let per_extra = match previous {
                    Some((earlier_count, earlier_tokens)) if count > earlier_count => format!(
                        "{:.0}",
                        f64::from(tokens - earlier_tokens) / (count - earlier_count) as f64
                    ),
                    _ => "—".to_string(),
                };
                println!("{count:>9}  {tokens:>7}  {per_extra:>9}  {latency:>6} ms  ok");
                previous = Some((count, tokens));
            }
            Err(reason) => println!("{count:>9}  {:>7}  {:>9}  {:>9}  {reason}", "—", "—", "—"),
        }
    }
}

/// Does a German order cost accuracy? The docs say English is the primary
/// training language, so this is worth knowing before the texts get polished.
fn order_language(client: &Client) {
    println!("\n== German against English, same situation ==");
    println!("{:<34}  {:<18}  {:<18}", "order", "German", "English");

    let pairs = [
        ("Geht alle nach Osten", "All of you, go east"),
        ("Bleibt beieinander", "Stay close together"),
        ("Verteilt euch", "Spread out"),
        ("Schont eure Kräfte", "Save your strength"),
        ("Geht nach Westen", "Go west"),
    ];

    for (german, english) in pairs {
        let left = ask(client, &build_request(&crowded_view(german)));
        let right = ask(client, &build_request(&crowded_view(english)));
        println!("{german:<34}  {:<18}  {:<18}", show(&left), show(&right));
    }
}

/// Does the ant's sight change its mind at all? An order about the neighbours
/// must answer differently when the neighbours are invisible — otherwise the
/// whole per-ant request is paid for nothing.
fn what_sight_contributes(client: &Client) {
    println!("\n== With sight against without ==");
    println!(
        "{:<34}  {:<18}  {:<18}",
        "order", "sees neighbours", "sees nothing"
    );

    for order in [
        "Bleibt beieinander",
        "Verteilt euch",
        "Geht dorthin, wo am meisten Platz ist",
        "Geht alle nach Osten",
    ] {
        let seeing = ask(client, &build_request(&crowded_view(order)));
        let blind = ask(client, &build_request(&blind_view(order)));
        println!("{order:<34}  {:<18}  {:<18}", show(&seeing), show(&blind));
    }

    println!(
        "\nThe crowd stands to the north, the wall to the west; south is open.\n\
         An order that takes the neighbours into account has to differ between the columns."
    );
}

fn show(result: &Result<(String, f32, u32, u32), String>) -> String {
    match result {
        Ok((choice, confidence, ..)) => format!("{choice} {confidence:.2}"),
        Err(reason) => reason.chars().take(16).collect(),
    }
}

/// Where does the acceptance threshold belong?
///
/// A fixed number cannot work: the options are generated per ant, so an ant in
/// a corner is judged against four of them and one in the open against nine.
/// This measures the distance from pure chance instead, and it uses orders that
/// should clearly be obeyed against orders that mean nothing to an ant — the
/// threshold belongs in the gap between the two groups.
fn where_the_threshold_belongs(client: &Client) {
    println!("\n== Where does the threshold belong? ==");
    println!(
        "{:<38} {:>2} {:>9} {:>7} {:>6} {:>10}",
        "order", "n", "choice", "p_top", "conf", "advantage"
    );

    let meaningful = [
        "Geht alle nach Osten",
        "Geht nach Süden",
        "Verteilt euch",
        "Bleibt beieinander",
        "Geht dorthin, wo am meisten Platz ist",
        "Schont eure Kräfte",
    ];
    let meaningless = [
        "",
        "Backt einen Kuchen",
        "Die Sonne scheint heute schön",
        "Sieben mal acht ist sechsundfünfzig",
    ];

    for order in meaningful {
        row(client, order, &Dir::ALL);
    }
    println!("{:-<76}", "");
    for order in meaningless {
        row(client, order, &Dir::ALL);
    }
    println!("{:-<76}", "");

    // The same order judged against four options instead of nine: an ant in a
    // corner. A fixed threshold would hold it to a different standard.
    let cornered = [Dir::North, Dir::East, Dir::NorthEast, Dir::Stay];
    for order in [
        "Geht alle nach Osten",
        "Verteilt euch",
        "Backt einen Kuchen",
    ] {
        row(client, order, &cornered);
    }

    println!(
        "\nadvantage = (p_top - 1/n) / (1 - 1/n): 0 means the model is guessing,\n\
         1 means certainty. The threshold goes between the two groups above."
    );
}

fn row(client: &Client, order: &str, options: &[Dir]) {
    let view = AntView {
        id: AntId(0),
        order,
        options: options.to_vec(),
        sightings: crowded_view(order).sightings,
    };

    let shown = if order.is_empty() {
        "(no order)"
    } else {
        order
    };
    let count = options.len();

    let reply = match ask_and_wait(client, &build_request(&view)) {
        Ok(reply) => reply,
        Err(error) => {
            println!("{shown:<38} {count:>2}  {error}");
            return;
        }
    };

    match reply.response.answers.get(STEP_QUESTION) {
        Some(Answer::Choice {
            choice,
            probabilities,
            confidence,
        }) => {
            let top = probabilities.get(choice).copied().unwrap_or(0.0);
            let chance = 1.0 / count as f32;
            let advantage = ((top - chance) / (1.0 - chance)).clamp(0.0, 1.0);
            println!(
                "{shown:<38} {count:>2} {choice:>9} {top:>7.3} {confidence:>6.2} {advantage:>10.3}"
            );
        }
        other => println!("{shown:<38} {count:>2}  unexpected: {other:?}"),
    }
}
