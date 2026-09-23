//! What one ant was asked, and what came back.
//!
//! The rest of the HUD shows totals, and totals are about running the game.
//! This window is about the model: one state, one option list, one distribution.
//! Three things become visible here that can otherwise only be asserted —
//!
//! * the options were **built from the board**, so nothing the ant cannot reach
//!   is on the list, and nothing off the list can win;
//! * `confidence` is the distance from pure chance, normalised by how many
//!   options there were — which is obvious the moment the distribution is next
//!   to it, and puzzling for as long as it is not;
//! * the state is tiny. Three cells of sight, a handful of sentences. That so
//!   little is enough is a statement about Jev.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::ants::components::AntId;
use crate::api::ENDPOINT_PATH;
use crate::decisions::{Exchange, Outcome};

/// Which ant the inspector is showing. `None` means no window.
#[derive(Resource, Default)]
pub struct Selected(pub Option<AntId>);

/// Which side of the exchange is on screen.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy)]
pub enum Tab {
    /// The exchange laid out in sentences, with the API name against each part.
    #[default]
    Read,
    /// The two bodies as JSON, exactly as they went out and came back.
    Raw,
}

const CHOSEN: egui::Color32 = egui::Color32::from_rgb(90, 190, 110);
const REFUSED: egui::Color32 = egui::Color32::from_rgb(224, 110, 96);
const QUIET: egui::Color32 = egui::Color32::from_rgb(130, 136, 148);

pub fn window(
    ctx: &egui::Context,
    selected: &mut Selected,
    tab: &mut Tab,
    exchange: Option<&Exchange>,
) {
    let Some(ant) = selected.0 else {
        return;
    };

    let mut open = true;
    egui::Window::new(format!("Ant {:02}", ant.0))
        .default_width(460.0)
        .default_pos(egui::pos2(24.0, 110.0))
        .open(&mut open)
        .vscroll(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(*tab == Tab::Read, "Reading").clicked() {
                    *tab = Tab::Read;
                }
                if ui
                    .selectable_label(*tab == Tab::Raw, "Request & response")
                    .clicked()
                {
                    *tab = Tab::Raw;
                }
            });
            ui.separator();

            match (exchange, *tab) {
                (Some(exchange), Tab::Read) => body(ui, exchange),
                (Some(exchange), Tab::Raw) => raw(ui, exchange),
                (None, _) => {
                    ui.label("Nothing asked of this ant yet.");
                    ui.label(
                        egui::RichText::new(
                            "Before the queen's first order the colony sleeps, and an ant that \
                         is busy pursuing an intent is not asked again until it is done.",
                        )
                        .small()
                        .color(QUIET),
                    );
                }
            }
        });

    if !open {
        selected.0 = None;
    }
}

/// The two bodies, as they really were.
///
/// This is the tab to open next to the API reference. Everything the other tab
/// says in sentences is here in the shape the documentation uses — and a little
/// more besides, because the response carries fields this game does not read
/// yet. Seeing them is how you find out they exist.
fn raw(ui: &mut egui::Ui, exchange: &Exchange) {
    ui.label(
        egui::RichText::new(format!("POST {ENDPOINT_PATH}"))
            .small()
            .monospace()
            .color(QUIET),
    );

    json_block(ui, "Request body", &exchange.request);

    match (&exchange.response, &exchange.outcome) {
        (Some(body), _) => json_block(ui, "Response body", body),
        (None, Outcome::Failed { reason }) => {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Response body").strong().small());
            ui.separator();
            ui.label(
                egui::RichText::new(format!("Nothing came back — {reason}"))
                    .small()
                    .color(REFUSED),
            );
        }
        (None, _) => {}
    }
}

fn json_block(ui: &mut egui::Ui, title: &str, body: &str) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(title).strong().small());
        if ui.small_button("Copy").clicked() {
            ui.ctx().copy_text(body.to_string());
        }
    });
    ui.separator();

    // A read-only code editor rather than a label: it selects, scrolls and
    // copies, which a label does not, and the edits go nowhere because the
    // buffer is a throwaway copy.
    let mut shown = body.to_string();
    ui.add(
        egui::TextEdit::multiline(&mut shown)
            .code_editor()
            .desired_rows(body.lines().count().min(24))
            .desired_width(f32::INFINITY),
    );
}

fn body(ui: &mut egui::Ui, exchange: &Exchange) {
    heading(ui, "What it sent", "state");
    if exchange.order.is_empty() {
        ui.label(
            egui::RichText::new("no order — the field is left out of the state entirely")
                .small()
                .color(QUIET),
        );
    } else {
        field(ui, "order_from_the_ant_queen");
        ui.label(format!("  \"{}\"", exchange.order));
    }
    if exchange.carrying {
        field(ui, "carrying");
        ui.label("  \"a fruit\"");
    }
    field(ui, "nearby");
    for sighting in &exchange.sightings {
        ui.label(egui::RichText::new(format!("  · {sighting}")).small());
    }
    if exchange.sightings.is_empty() {
        ui.label(egui::RichText::new("  · nothing within sight").small());
    }
    ui.label(
        egui::RichText::new(
            "The state also carries a constant `vision` line saying how far this ant sees.",
        )
        .small()
        .color(QUIET),
    );

    ui.add_space(8.0);
    heading(ui, "What it was asked", "questions.step.instructions");
    ui.label(egui::RichText::new(&exchange.instructions).italics());
    ui.label(
        egui::RichText::new(
            "Asked as \"type\": \"choice\" under the key `step`. Questions are evaluated \
             independently against the same state, so a second one could be added here \
             without chaining it onto this one.",
        )
        .small()
        .color(QUIET),
    );

    ui.add_space(8.0);
    heading(
        ui,
        &format!("The options it was given ({})", exchange.offered.len()),
        "questions.step.criteria",
    );
    for (key, description) in &exchange.offered {
        ui.label(
            egui::RichText::new(format!("{key} — {description}"))
                .small()
                .monospace(),
        );
    }
    ui.label(
        egui::RichText::new(
            "This list is built from what the ant can see, every request. \
             An answer outside it is refused, so a fruit that is not there \
             cannot be fetched.",
        )
        .small()
        .color(QUIET),
    );

    ui.add_space(8.0);
    heading(ui, "What came back", "answers.step");
    match &exchange.outcome {
        Outcome::Taken {
            chosen,
            confidence,
            probabilities,
            latency_ms,
            input_tokens,
        } => {
            field(ui, "choice");
            ui.label(egui::RichText::new(format!("  \"{chosen}\"")).small());
            field(ui, "probabilities");
            distribution(ui, probabilities, Some(chosen), &exchange.offered);
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!(
                    "confidence {confidence:.2} · usage.input_tokens {input_tokens}"
                ))
                .small(),
            );
            // The round trip is ours, measured here. Nothing in the response
            // reports it, and labelling it like an API field would be a lie.
            ui.label(
                egui::RichText::new(format!("{latency_ms} ms, measured around the request"))
                    .small()
                    .color(QUIET),
            );
            ui.label(
                egui::RichText::new(format!(
                    "Confidence is the distance from pure chance over {} options, \
                     not the top probability.",
                    exchange.offered.len()
                ))
                .small()
                .color(QUIET),
            );
        }
        Outcome::Refused {
            reason,
            probabilities,
            latency_ms,
        } => {
            ui.label(
                egui::RichText::new(format!("Thrown away — {reason}"))
                    .strong()
                    .color(REFUSED),
            );
            ui.label(
                egui::RichText::new(format!(
                    "The answer arrived after {latency_ms} ms and was paid for. \
                     The ant did not act this round; nothing decided in its place."
                ))
                .small()
                .color(QUIET),
            );
            ui.add_space(4.0);
            field(ui, "probabilities");
            distribution(ui, probabilities, None, &exchange.offered);
        }
        Outcome::Failed { reason } => {
            ui.label(
                egui::RichText::new(format!("Nothing came back — {reason}"))
                    .strong()
                    .color(REFUSED),
            );
        }
    }
}

/// The distribution as bars, highest first.
///
/// Every key is checked against the option list as it is drawn. A key that was
/// never offered is marked, because that is the exact moment the type safety
/// earns its keep — and with a real option list it should never appear.
fn distribution(
    ui: &mut egui::Ui,
    probabilities: &[(String, f32)],
    chosen: Option<&String>,
    offered: &[(String, String)],
) {
    if probabilities.is_empty() {
        ui.label(
            egui::RichText::new("no distribution in the answer")
                .small()
                .color(QUIET),
        );
        return;
    }

    for (key, weight) in probabilities {
        let is_chosen = chosen == Some(key);
        let was_offered = offered.iter().any(|(offered_key, _)| offered_key == key);

        let mut text = format!("{key}  {weight:.2}");
        if is_chosen {
            text = format!("→ {text}");
        }
        if !was_offered {
            text = format!("{text}  (never offered)");
        }

        let fill = if !was_offered {
            REFUSED
        } else if is_chosen {
            CHOSEN
        } else {
            QUIET
        };

        ui.add(
            egui::ProgressBar::new(*weight)
                .desired_width(380.0)
                .fill(fill)
                .text(egui::RichText::new(text).small()),
        );
    }
}

/// A section heading with the name the API gives that part.
///
/// The English is for reading, the monospace is what you look up in the docs at
/// <https://docs.typesafe.ai>. Both together are the point: the window is about
/// an ant, but what it shows is a request and a response.
fn heading(ui: &mut egui::Ui, text: &str, api_field: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text).strong().small());
        ui.label(
            egui::RichText::new(api_field)
                .small()
                .monospace()
                .color(QUIET),
        );
    });
    ui.separator();
}

/// One field name inside a section, above the value it holds.
fn field(ui: &mut egui::Ui, name: &str) {
    ui.label(egui::RichText::new(name).small().monospace().color(QUIET));
}
