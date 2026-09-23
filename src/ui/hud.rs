//! What the model is doing, made visible.
//!
//! The intent and confidence over every ant is the actual demo: without it, a
//! colony steered by Jev looks exactly like a colony following pheromones.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::ants::BODY;
use crate::ants::components::{AntId, LastDecision};
use crate::config::{ANT_COUNT, CELL_SIZE, THINK_INTERVAL};
use crate::decisions::{DecisionStats, Link};

use crate::world::grid::{Dir, Grid};
use crate::world::render::View;

use super::DebugLayer;
use super::inspector::Selected;

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

/// The ant the inspector is open on. Blue because nothing else on the board is
/// — the confidence bands run green through yellow to red, and an unasked ant
/// is grey.
const SELECTED_RGB: (u8, u8, u8) = (86, 148, 246);

/// The lamp in the top right corner: is there a line to Jev at all?
///
/// Green only means the last request came back. It says nothing about whether
/// the answer was any good — that is what the discarded count is for, and
/// telling the two apart is worth more than one combined light would be.
fn link_lamp(ui: &mut egui::Ui, link: &Link) {
    let rgb = match link {
        Link::Live => (90, 205, 110),
        Link::Down(_) => (224, 88, 78),
        Link::Untried => (128, 134, 146),
    };

    let detail = link.detail();
    // The `horizontal` around it is load-bearing, not decoration: a bare
    // `with_layout` takes the whole available height for its cross axis, the
    // panel grows to fit it, and next frame there is that much more available
    // again. The bar grew by 45 px a frame and swallowed the board.
    ui.horizontal(|ui| {
        // Right to left, so the dot ends up hard against the corner and the
        // words sit to its left.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, dot) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            let colour = egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2);
            ui.painter().circle_filled(rect.center(), 5.0, colour);
            // A ring around it, so the state reads without relying on colour alone.
            ui.painter().circle_stroke(
                rect.center(),
                7.0,
                egui::Stroke::new(1.0, colour.gamma_multiply(0.5)),
            );
            dot.on_hover_text(&detail);

            ui.label(egui::RichText::new(link.short()).small().color(colour))
                .on_hover_text(&detail);
        });
    });
}

/// Everything the bar says about the **model**, as opposed to about the colony.
/// Grouped because they are one story: who is deciding, whether the line to them
/// is up, and what it has cost so far.
pub struct ModelState<'a> {
    pub stats: &'a DecisionStats,
    pub source: &'a str,
    pub awake: bool,
    pub link: &'a Link,
}

pub fn top_bar(
    ui: &mut egui::Ui,
    model: &ModelState<'_>,
    debug_on: bool,
    stored: u32,
    on_the_board: usize,
) {
    let ModelState {
        stats,
        source,
        awake,
        link,
    } = model;
    let awake = *awake;
    egui::Panel::top("hud").show(ui, |ui| {
        ui.add_space(6.0);
        // Its own row, always in the same corner — a lamp one has to look for is
        // not a lamp. The numbers below it wrap; this must not move with them.
        link_lamp(ui, link);
        ui.horizontal_wrapped(|ui| {
            // The colony first: that is the game. The model's numbers after it.
            ui.label(egui::RichText::new(format!("{stored} stored")).strong());
            ui.label(format!("· {on_the_board} fruits on the board"));
            if stored > 0 {
                // Requests on *this* level over fruit from this level. The
                // session totals to the right are a different question and must
                // not be mixed into this one.
                ui.label(format!(
                    "· {:.1} requests per fruit",
                    f64::from(stats.this_level) / f64::from(stored)
                ));
            }
            ui.separator();
            // Naming the source before the queen has spoken would be a lie:
            // nothing is being asked yet, and nothing has been paid for.
            ui.label(if awake {
                format!("Deciding: {source}")
            } else {
                "Deciding: nobody yet — the queen has not spoken".to_string()
            });
            ui.separator();
            ui.label(format!(
                "{} decisions from Jev, {} discarded",
                stats.from_jev, stats.discarded
            ));
            ui.separator();
            ui.label(format!("{} ms average", stats.average_latency_ms()));
            ui.separator();
            // The bill is the session's, not the level's — picking another
            // board does not give the money back.
            ui.label(format!(
                "{} input tokens · ${:.4} this session · ${:.2}/h",
                stats.input_tokens,
                stats.cost_usd(),
                stats.projected_usd_per_hour(ANT_COUNT, THINK_INTERVAL)
            ));
            ui.separator();
            ui.label(
                egui::RichText::new(if debug_on {
                    "F3 hides the intents · F2 the question · click an ant to see its request"
                } else {
                    "F3 shows the intents · F2 the question · click an ant to see its request"
                })
                .weak(),
            );
        });
        ui.add_space(6.0);
    });
}

/// The colour of the four compass words. Dim on purpose: they are a legend for
/// the board, not something on it.
const COMPASS_RGB: egui::Color32 = egui::Color32::from_rgb(146, 138, 122);
/// How far outside the board edge a word sits, in logical pixels. The air for
/// it is what `View::fit` keeps free around every board.
const COMPASS_GAP: f32 = 5.0;

/// Which way is north, written into the air around the board.
///
/// The queen says "go east" and the ant is asked between options spelled with
/// the same eight words. Which way that is on screen was until now something
/// the player had to work out by watching where the ants went — and guessing
/// wrong turns a perfectly good order into an apparent failure of the model.
///
/// Drawn from `View`, like the intent labels, so the words stay put whatever a
/// level zooms to.
pub fn compass(ui: &mut egui::Ui, view: &View, grid: Grid) {
    let painter = ui.painter();
    // Half the board in world units: the middle of each edge, measured from the
    // centre the board is drawn around.
    let half = Vec2::new(grid.width as f32, grid.height as f32) * CELL_SIZE * 0.5;

    for (direction, edge, nudge, align) in [
        (
            Dir::North,
            Vec2::new(0.0, half.y),
            egui::vec2(0.0, -COMPASS_GAP),
            egui::Align2::CENTER_BOTTOM,
        ),
        (
            Dir::South,
            Vec2::new(0.0, -half.y),
            egui::vec2(0.0, COMPASS_GAP),
            egui::Align2::CENTER_TOP,
        ),
        (
            Dir::West,
            Vec2::new(-half.x, 0.0),
            egui::vec2(-COMPASS_GAP, 0.0),
            egui::Align2::RIGHT_CENTER,
        ),
        (
            Dir::East,
            Vec2::new(half.x, 0.0),
            egui::vec2(COMPASS_GAP, 0.0),
            egui::Align2::LEFT_CENTER,
        ),
    ] {
        let at = view.to_screen(edge);
        painter.text(
            egui::pos2(at.x, at.y) + nudge,
            align,
            // The ant's own word for it, not a second vocabulary beside it.
            direction.spoken(),
            egui::FontId::proportional(12.0),
            COMPASS_RGB,
        );
    }
}

/// Intent and confidence above each ant.
///
/// The position comes from `View`, the same mapping the camera and the click
/// picker use. Working it out here as well is what would let the labels drift
/// off the ants the moment a level zoomed differently.
pub fn intent_overlay(ui: &mut egui::Ui, view: &View, ants: &super::Ants<'_, '_>) {
    let painter = ui.painter();
    // Text does not shrink with the board; a label at half size is unreadable
    // whatever the board is doing.
    let above = 14.0;
    for (_, transform, last, _) in ants {
        let Some(action) = last.action else {
            continue; // has not decided anything yet
        };

        let (text, rgb) = match last.confidence {
            Some(confidence) => (
                format!("{} {confidence:.2}", action.label()),
                confidence_rgb(confidence),
            ),
            None => (format!("{} classic", action.label()), CLASSIC_RGB),
        };

        let at = view.to_screen(transform.translation.truncate());
        painter.text(
            egui::pos2(at.x, at.y - above),
            egui::Align2::CENTER_BOTTOM,
            text,
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(rgb.0, rgb.1, rgb.2),
        );
    }
}

/// The same colour on the ant itself, so the picture reads even without the
/// text — and blue on the one the inspector is open on.
///
/// Selection wins over the confidence colour, and over the debug layer being
/// off. Losing one ant's coarse colour costs nothing: its whole distribution is
/// on screen in the window, which is rather more than a colour can say.
pub fn tint_ants(
    layer: Res<DebugLayer>,
    selected: Res<Selected>,
    mut ants: Query<(&AntId, &LastDecision, &mut Sprite)>,
) {
    for (id, last, mut sprite) in &mut ants {
        let (red, green, blue) = if selected.0 == Some(*id) {
            SELECTED_RGB
        } else if !layer.0 {
            sprite.color = BODY;
            continue;
        } else {
            match last.confidence {
                Some(confidence) => confidence_rgb(confidence),
                None => CLASSIC_RGB,
            }
        };
        sprite.color = Color::srgb_u8(red, green, blue);
    }
}
