//! The dialog that asks for a key when the game started without one.
//!
//! Before this, a player who had not put a key in `.env` got a colony that sat
//! in the nest and a red lamp telling them to edit a file and start again. The
//! dialog asks instead — and asks for **this run only**: what is typed here
//! goes into the client and nowhere else, so the game never becomes a place a
//! secret is quietly kept (`decisions::KeyPrompt`).
//!
//! There is no way past it but a key. A colony that cannot ask anyone is not a
//! reduced version of this game, it is an empty board — offering it as a choice
//! would only let a player wander into one.

use bevy::prelude::*;
use bevy_egui::egui;

/// Where the key is typed, kept apart from the client for the same reason the
/// queen's draft is kept apart from her order: what is half-typed is not yet
/// meant.
#[derive(Resource, Default)]
pub struct KeyDraft {
    pub text: String,
    /// A key is a secret, so it is dotted out. It can be turned around all the
    /// same: a paste that went wrong is invisible otherwise, and the only other
    /// way to find out is to spend a request on it.
    pub visible: bool,
    /// The field takes the cursor once, when the dialog opens. Doing it every
    /// frame would fight the player for it the moment they click elsewhere.
    focused: bool,
}

/// Where to get one. Named rather than hidden behind "get a key": since the
/// dialog cannot be waved away, a player without an account needs the address
/// right here.
const CONSOLE_URL: &str = "https://console.typesafe.ai/keys";

/// The key, once the player has committed to one. `None` means they are still
/// typing — this dialog has no other answer.
pub fn dialog(ctx: &egui::Context, draft: &mut KeyDraft) -> Option<String> {
    // A modal rather than a window: its backdrop blocks the board and the
    // queen's bar behind it, which is the honest thing to do — until this is
    // answered, typing an order would go to a colony that cannot ask anyone.
    egui::Modal::new(egui::Id::new("api_key_prompt"))
        .show(ctx, |ui| {
            ui.set_width(460.0);
            let mut given = None;

            ui.heading("The colony has no one to think with");
            ui.add_space(6.0);
            ui.label(
                "Every ant asks TypeSafe Jev what to do next. Without a key nothing is \
                 asked and nothing moves — the ants stay in the nest.",
            );
            ui.add_space(10.0);

            ui.label(egui::RichText::new("API key").small().strong());
            let field = ui.add(
                egui::TextEdit::singleline(&mut draft.text)
                    .password(!draft.visible)
                    .hint_text("paste your TypeSafe key")
                    .desired_width(f32::INFINITY),
            );
            if !draft.focused {
                field.request_focus();
                draft.focused = true;
            }
            let entered =
                field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.checkbox(&mut draft.visible, egui::RichText::new("Show it").small());
                ui.add_space(8.0);
                ui.hyperlink_to(egui::RichText::new(CONSOLE_URL).small(), CONSOLE_URL);
            });

            ui.add_space(10.0);
            // The one way out. An empty field is not an answer, so the button
            // is dead until there is something in it.
            let has_key = !draft.text.trim().is_empty();
            if ui
                .add_enabled(has_key, egui::Button::new("Use this key"))
                .clicked()
                || (entered && has_key)
            {
                given = Some(draft.text.trim().to_string());
                // Gone from here the moment it is handed over. The client holds
                // the only copy from now on.
                draft.text.clear();
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Used for this run only. It is not saved anywhere — not on disk, not \
                     in a save game — so the next start asks again. Put it in .env as \
                     TYPESAFE_API_KEY to be spared the typing.",
                )
                .small()
                .weak(),
            );

            // Said here because it is true here and nowhere else. A page cannot
            // reach the API itself, so the key takes a detour through whatever
            // serves this game, and a player deciding whether to paste a secret
            // should not have to read the README to find that out. Running the
            // game from a checkout keeps the key between the terminal and the
            // API — `api::DEFAULT_BASE_URL`.
            #[cfg(target_arch = "wasm32")]
            ui.label(
                egui::RichText::new(
                    "In the browser it also passes through this site's proxy on its way \
                     to the API. The proxy only forwards and stores nothing — but it is \
                     someone else's machine, and running the game locally avoids it.",
                )
                .small()
                .weak(),
            );

            given
        })
        .inner
}
