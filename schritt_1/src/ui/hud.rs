//! What the model is doing, made visible.
//!
//! The intent and confidence over every ant is the actual demo: without it, a
//! colony steered by Jev looks exactly like a colony following pheromones.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::ants::BODY;
use crate::ants::components::LastDecision;
use crate::config::{ANT_COUNT, THINK_INTERVAL};
use crate::decisions::DecisionStats;

use super::DebugLayer;

/// Green when the model is sure, red when it only just cleared the threshold.
///
/// The bands follow the measured range: a plain compass order scores around
/// 0.99, a nuanced one ("Verteilt euch") between 0.29 and 0.44, and anything
/// below `MIN_CONFIDENCE` never becomes a move at all.
fn confidence_rgb(confidence: f32) -> (u8, u8, u8) {
    if confidence >= 0.60 {
        (120, 220, 130)
    } else if confidence >= 0.35 {
        (232, 208, 110)
    } else {
        (228, 130, 110)
    }
}

/// The classic rules have no confidence, so they get their own colour rather
/// than a made-up number.
const CLASSIC_RGB: (u8, u8, u8) = (140, 150, 170);

pub fn top_bar(ui: &mut egui::Ui, stats: &DecisionStats, source: &str, debug_on: bool) {
    egui::Panel::top("hud").show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("Deciding: {source}")).strong());
            ui.separator();
            ui.label(format!(
                "{} steps from Jev, {} classic",
                stats.from_jev, stats.from_classic
            ));
            ui.separator();
            ui.label(format!("{} ms average", stats.average_latency_ms()));
            ui.separator();
            ui.label(format!(
                "{} input tokens · ${:.4} so far · ${:.2}/h",
                stats.input_tokens,
                stats.cost_usd(),
                stats.projected_usd_per_hour(ANT_COUNT, THINK_INTERVAL)
            ));
            ui.separator();
            ui.label(
                egui::RichText::new(if debug_on {
                    "F3 hides the intents"
                } else {
                    "F3 shows the intents"
                })
                .weak(),
            );
        });
        ui.add_space(6.0);
    });
}

/// Intent and confidence above each ant.
///
/// World units are logical pixels and the camera sits at the origin, so the
/// screen position is the viewport centre plus the ant's offset, with `y`
/// flipped. No camera projection needed, and none that could get out of step.
pub fn intent_overlay(
    ui: &mut egui::Ui,
    centre: egui::Pos2,
    ants: &Query<(&Transform, &LastDecision)>,
) {
    let painter = ui.painter();
    for (transform, last) in ants {
        let Some(direction) = last.dir else {
            continue; // has not decided anything yet
        };

        let (text, rgb) = match last.confidence {
            Some(confidence) => (
                format!("{} {confidence:.2}", direction.spoken()),
                confidence_rgb(confidence),
            ),
            None => (format!("{} classic", direction.spoken()), CLASSIC_RGB),
        };

        painter.text(
            egui::pos2(
                centre.x + transform.translation.x,
                centre.y - transform.translation.y - 14.0,
            ),
            egui::Align2::CENTER_BOTTOM,
            text,
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2),
        );
    }
}

/// The same colour on the ant itself, so the picture reads even without the text.
pub fn tint_by_confidence(layer: Res<DebugLayer>, mut ants: Query<(&LastDecision, &mut Sprite)>) {
    for (last, mut sprite) in &mut ants {
        sprite.color = if !layer.0 {
            BODY
        } else {
            let (red, green, blue) = match last.confidence {
                Some(confidence) => confidence_rgb(confidence),
                None => CLASSIC_RGB,
            };
            Color::srgb_u8(red, green, blue)
        };
    }
}
