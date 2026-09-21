//! The only input the game has: a text field.
//!
//! No unit selection, no buttons per ant. What the player types is handed to
//! the model word for word — translating it would be a decision in itself, and
//! the ants are supposed to interpret the order, not a cleaned-up version of it.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::config::REMEMBERED_ORDERS;
use crate::decisions::QueenOrder;

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

/// The bar at the bottom. Takes the root `Ui` rather than making its own, so
/// the HUD above and this panel lay out against the same space.
pub fn bottom_bar(
    ui: &mut egui::Ui,
    draft: &mut OrderDraft,
    order: &mut QueenOrder,
    history: &mut OrderHistory,
) {
    egui::Panel::bottom("queen_order").show(ui, |ui| {
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("The queen says:");

            let width = (ui.available_width() - 170.0).max(120.0);
            let field = ui.add(
                egui::TextEdit::singleline(&mut draft.0)
                    .hint_text("tell the colony what to do")
                    .desired_width(width),
            );
            let entered =
                field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

            // An empty field says nothing; silence is what the other button is for.
            if (ui.button("Say").clicked() || entered) && !draft.0.trim().is_empty() {
                let said = draft.0.trim().to_string();
                history.remember(&said);
                order.0 = said;
                draft.0.clear();
                // Ready for the next order without reaching for the mouse.
                field.request_focus();
            }

            if ui.button("Say nothing").clicked() {
                draft.0.clear();
                order.0.clear();
            }
        });

        ui.add_space(4.0);
        let status = if order.0.is_empty() {
            "No order — every ant decides on its own.".to_string()
        } else {
            format!("The colony has heard: \"{}\"", order.0)
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
}
