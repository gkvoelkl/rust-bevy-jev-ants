//! The bench: the question itself, and the answers that were thrown away.
//!
//! Both halves are here because both are about the wording. `ANTS.md` §4.5 calls
//! the texts an asset rather than code, and the reason is right next to it —
//! most thrown-away answers are not a fault of the model but of the question,
//! and you only see that if you can reword and watch the next round.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::decisions::questions::{QUESTIONS_PATH, Questions};
use crate::decisions::{DiscardLog, Exchange, Outcome};

/// The question text being typed, kept apart from the one in force.
///
/// Same reason as `OrderDraft`: binding the widget straight to the resource
/// would mark it changed on every frame, and every ant would be asked again
/// sixty times a second.
#[derive(Resource, Default)]
pub struct QuestionDraft {
    pub text: String,
    /// What the last load or commit did, shown under the field.
    pub note: String,
}

impl QuestionDraft {
    pub fn starting_from(questions: &Questions) -> Self {
        Self {
            text: questions.step.clone(),
            note: String::new(),
        }
    }
}

const QUIET: egui::Color32 = egui::Color32::from_rgb(130, 136, 148);
const REFUSED: egui::Color32 = egui::Color32::from_rgb(224, 110, 96);

/// Returns the new question text when the player committed one.
pub fn window(
    ctx: &egui::Context,
    open: &mut bool,
    draft: &mut QuestionDraft,
    in_force: &str,
    log: &DiscardLog,
) -> Option<String> {
    let mut committed = None;

    egui::Window::new("The question")
        .default_width(430.0)
        .default_pos(egui::pos2(840.0, 110.0))
        .open(open)
        .vscroll(true)
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new(
                    "The text every ant is asked, against its own option list. \
                     Reword it and the next round answers differently — that is \
                     the experiment this game is for.",
                )
                .small()
                .color(QUIET),
            );
            ui.add_space(6.0);

            ui.add(
                egui::TextEdit::multiline(&mut draft.text)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );

            ui.horizontal(|ui| {
                let changed = draft.text.trim() != in_force.trim();
                if ui
                    .add_enabled(
                        changed && !draft.text.trim().is_empty(),
                        egui::Button::new("Ask with this"),
                    )
                    .clicked()
                {
                    committed = Some(draft.text.trim().to_string());
                    draft.note = "In force — every ant is being asked again.".to_string();
                }
                if ui.button("Reload from file").clicked() {
                    match Questions::load() {
                        Ok(loaded) => {
                            draft.text = loaded.step.clone();
                            committed = Some(loaded.step);
                            draft.note = format!("Read from {QUESTIONS_PATH}.");
                        }
                        Err(reason) => draft.note = reason,
                    }
                }
            });

            if !draft.note.is_empty() {
                ui.label(egui::RichText::new(&draft.note).small().color(QUIET));
            }
            ui.label(
                egui::RichText::new(format!(
                    "Edits here live until the game closes. {QUESTIONS_PATH} is what survives."
                ))
                .small()
                .color(QUIET),
            );

            ui.add_space(12.0);
            ui.label(egui::RichText::new("Thrown away").strong().small());
            ui.separator();
            discards(ui, log);
        });

    committed
}

/// The last few answers that never became a move, with the reason.
fn discards(ui: &mut egui::Ui, log: &DiscardLog) {
    if log.0.is_empty() {
        ui.label(
            egui::RichText::new("Nothing thrown away yet.")
                .small()
                .color(QUIET),
        );
        return;
    }

    for exchange in &log.0 {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!("Ant {:02} — {}", exchange.ant.0, why(exchange)))
                .small()
                .color(REFUSED),
        );
        // The top of the distribution says more than the reason does: a refusal
        // for being close to chance looks like three options within a hair of
        // each other, and that is visible at a glance.
        let top: Vec<String> = exchange
            .probabilities()
            .iter()
            .take(3)
            .map(|(key, weight)| format!("{key} {weight:.2}"))
            .collect();
        if !top.is_empty() {
            ui.label(
                egui::RichText::new(format!("   {}", top.join("  ·  ")))
                    .small()
                    .monospace()
                    .color(QUIET),
            );
        }
    }
}

fn why(exchange: &Exchange) -> &str {
    match &exchange.outcome {
        Outcome::Refused { reason, .. } | Outcome::Failed { reason } => reason,
        Outcome::Taken { .. } => "used after all",
    }
}
