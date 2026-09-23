//! The only input the game has: a text field.
//!
//! No unit selection, no buttons per ant. What the player types is handed to
//! the model word for word — translating it would be a decision in itself, and
//! the ants are supposed to interpret the order, not a cleaned-up version of it.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::config::REMEMBERED_ORDERS;

/// What is being typed but has not been said yet. Keeping it apart from
/// `QueenOrder` means the colony does not react to every keystroke.
#[derive(Resource, Default)]
pub struct OrderDraft(pub String);

/// The orders given so far, newest first. The queen should be able to see what
/// she has already tried — most of the work in this game is rewording.
#[derive(Resource, Default)]
pub struct OrderHistory(pub Vec<String>);

impl OrderHistory {
    pub fn starting_with(order: &str) -> Self {
        let mut history = Self::default();
        history.remember(order);
        history
    }

    fn remember(&mut self, order: &str) {
        if order.is_empty() {
            return;
        }
        // An order given twice moves back to the top instead of appearing twice.
        self.0.retain(|earlier| earlier != order);
        self.0.insert(0, order.to_string());
        self.0.truncate(REMEMBERED_ORDERS);
    }
}

/// What the queen did, if anything.
pub enum OrderChange {
    Say(String),
    Silence,
}

/// What the bottom bar reports back. Two independent things happen down there —
/// an order and a change of level — and a turn can carry either.
#[derive(Default)]
pub struct BarResult {
    pub order: Option<OrderChange>,
    /// `None` is the open field; `Some(n)` the nth level on offer.
    pub level: Option<Option<usize>>,
    /// The board already being played, started again from the top.
    pub restart: bool,
}

/// The bar at the bottom. Takes the root `Ui` rather than making its own, so
/// the HUD above and this panel lay out against the same space.
///
/// It **reads** the current order and reports a change rather than writing it.
/// Writing through a `ResMut` every frame would mark the order as changed every
/// frame, and every ant would then be asked again sixty times a second — which
/// is exactly what happened before this was fixed.
/// What the picker needs to draw itself: what is on offer, and what is running.
pub struct LevelList<'a> {
    pub names: &'a [String],
    /// `None` means the open field is the one being played.
    pub playing: Option<&'a str>,
}

/// The task, when a board sets one. The open field has none, and then nothing
/// of this is drawn.
pub struct Task<'a> {
    pub briefing: &'a str,
    pub stored: u32,
    pub target: u32,
    pub solved: bool,
}

pub fn bottom_bar(
    ui: &mut egui::Ui,
    draft: &mut OrderDraft,
    order: &str,
    awake: bool,
    task: Option<&Task<'_>>,
    levels: &LevelList<'_>,
    history: &mut OrderHistory,
) -> BarResult {
    let mut result = BarResult::default();
    egui::Panel::bottom("queen_order").show(ui, |ui| {
        ui.add_space(8.0);

        // Picking a level starts it, there and then. No confirmation: trying
        // the same board again after a sentence that did not work is the whole
        // loop, and a dialog in the middle of it would be one click of friction
        // per attempt.
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new("Level").small().strong());
            if ui
                .selectable_label(levels.playing.is_none(), "Open field")
                .clicked()
            {
                result.level = Some(None);
            }
            for (index, name) in levels.names.iter().enumerate() {
                let current = levels.playing == Some(name.as_str());
                if ui
                    .selectable_label(current, format!("{}. {name}", index + 1))
                    .clicked()
                {
                    result.level = Some(Some(index));
                }
            }

            ui.separator();
            // The picker above would do this too — the lit entry answers a click
            // like any other — but only once you have worked out which entry is
            // lit. Saying a second sentence on the board in front of you is the
            // loop this game is for, and it should cost one click without
            // reading the row first.
            if ui
                .button("Restart")
                .on_hover_text("Same board. Fruit back, colony asleep in the nest.")
                .clicked()
            {
                result.restart = true;
            }
        });
        ui.add_space(6.0);

        if let Some(task) = task {
            if task.solved {
                ui.label(
                    egui::RichText::new(format!(
                        "Solved — {} of {} fruits home.",
                        task.stored, task.target
                    ))
                    .strong()
                    .color(egui::Color32::from_rgb(110, 210, 130)),
                );
            } else {
                ui.label(egui::RichText::new(task.briefing).strong());
                ui.label(
                    egui::RichText::new(format!("{} of {} fruits home.", task.stored, task.target))
                        .small(),
                );
            }
            ui.add_space(6.0);
        }

        ui.horizontal(|ui| {
            ui.label("The queen says:");

            let width = (ui.available_width() - 170.0).max(120.0);
            let field = ui.add(
                egui::TextEdit::singleline(&mut draft.0)
                    .hint_text(if awake {
                        "tell the colony what to do"
                    } else {
                        "say the first word — until then nothing happens"
                    })
                    .desired_width(width),
            );
            let entered =
                field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

            // An empty field says nothing; silence is what the other button is for.
            if (ui.button("Say").clicked() || entered) && !draft.0.trim().is_empty() {
                let said = draft.0.trim().to_string();
                history.remember(&said);
                result.order = Some(OrderChange::Say(said));
                draft.0.clear();
                // Ready for the next order without reaching for the mouse.
                field.request_focus();
            }

            if ui.button("Say nothing").clicked() {
                draft.0.clear();
                result.order = Some(OrderChange::Silence);
            }
        });

        ui.add_space(4.0);
        // Three states, not two: never spoken at all is not the same as having
        // taken an order back.
        let status = if !awake {
            "The colony waits in the nest. Nothing is asked until the queen speaks.".to_string()
        } else if order.is_empty() {
            "No order — every ant decides on its own.".to_string()
        } else {
            format!("The colony has heard: \"{order}\"")
        };
        ui.label(egui::RichText::new(status).small());

        if !history.0.is_empty() {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("Said before").small().weak());
            for earlier in &history.0 {
                ui.label(egui::RichText::new(format!("· {earlier}")).small().weak());
            }
        }

        ui.add_space(8.0);
    });

    result
}
