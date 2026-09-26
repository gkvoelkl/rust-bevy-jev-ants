// The offline tools — `--probe`, `--compare`, `--measure` — and the tests are
// native only, and they are what several corners of the API client and the grid
// exist for. In a web build those corners are genuinely unreachable, and saying
// so nine times per compile buries the warnings that would matter.
#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

mod ants;
mod api;
mod config;
mod decisions;
mod level;
mod ui;
mod world;

#[cfg(not(target_arch = "wasm32"))]
mod compare;
#[cfg(not(target_arch = "wasm32"))]
mod measure;
#[cfg(not(target_arch = "wasm32"))]
mod probe;

use bevy::prelude::*;

use decisions::{DecisionsFrom, DecisionsPlugin, QueenOrder};
use ui::queen_input::OrderHistory;

fn main() {
    // `cargo run -- --probe` fires one real request and prints the answer,
    // without starting the game.
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args().any(|argument| argument == "--probe") {
        probe::run();
        return;
    }

    // `cargo run -- --compare [jev]` measures whether the pheromones pay off.
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args().any(|argument| argument == "--compare") {
        compare::run(std::env::args().any(|argument| argument == "jev"));
        return;
    }

    // `cargo run -- --measure` answers the open questions from the concept.
    #[cfg(not(target_arch = "wasm32"))]
    if std::env::args().any(|argument| argument == "--measure") {
        measure::run();
        return;
    }

    let first_order = order_from_args();

    // `cargo run -- --scenario plank` plays a task board instead of the open
    // field. It has to be inserted before `WorldPlugin`, which reads it to
    // decide the shape of the whole world.
    let scenario = match named_arg("--scenario") {
        Some(name) => match world::scenario::Scenario::load(&name) {
            Ok(scenario) => Some(scenario),
            Err(reason) => {
                eprintln!("{reason}");
                eprintln!("Playing the open field instead.");
                None
            }
        },
        None => None,
    };

    let mut app = App::new();
    if let Some(scenario) = scenario {
        app.insert_resource(scenario);
    }
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Ant Colony — Step 1".into(),
            resolution: (1300u32, 860u32).into(),
            // A window that hides behind the editor is a window nobody watches,
            // and watching is the point. Meaningless in a browser tab, where
            // the canvas below takes over.
            #[cfg(not(target_arch = "wasm32"))]
            window_level: bevy::window::WindowLevel::AlwaysOnTop,
            // The canvas `index.html` puts up, kept at the size of the box
            // around it. Without the selector Bevy appends a second canvas of
            // its own and the page has two.
            #[cfg(target_arch = "wasm32")]
            canvas: Some("#game".to_string()),
            #[cfg(target_arch = "wasm32")]
            fit_canvas_to_parent: true,
            // Off, so that pasting works — and pasting is how a key gets into
            // the dialog. Bevy's default has winit call `preventDefault()` on
            // every `keydown` in the browser, and the paste event a browser
            // fires is *the default action* of Ctrl/Cmd+V. Prevent that and no
            // paste event is ever raised, which is what `bevy_egui` listens for
            // on the document. The field then simply ignores the shortcut, with
            // nothing in the console to say why.
            //
            // What it costs: the browser's own shortcuts work again over the
            // canvas — F3 may open the find bar where the game means the intent
            // layer, a right-click shows the browser menu, and Tab moves focus
            // off the canvas until it is clicked again. All of that is worth
            // less than being able to paste the key the game cannot start
            // without.
            #[cfg(target_arch = "wasm32")]
            prevent_default_event_handling: false,
            ..default()
        }),
        ..default()
    }))
    // An order given on the command line counts as already said: it is in
    // effect, it shows up in the list, and the field starts empty. It also
    // wakes the colony at once — without one the ants sit in the nest until
    // the queen types her first order, and nothing is asked of Jev before
    // that.
    .insert_resource(OrderHistory::starting_with(&first_order))
    .insert_resource(QueenOrder(first_order))
    .add_plugins((
        world::WorldPlugin,
        DecisionsPlugin {
            source: DecisionsFrom::Jev,
        },
        ants::AntsPlugin,
        level::LevelPlugin,
        ui::UiPlugin,
    ))
    .run();
}

/// `cargo run -- --order "Geht alle nach Osten"` sets the first order, which is
/// otherwise given through the text field.
fn order_from_args() -> String {
    named_arg("--order").unwrap_or_default()
}

/// The value after `flag` on the command line, if it is there.
fn named_arg(flag: &str) -> Option<String> {
    let mut arguments = std::env::args();
    while let Some(argument) = arguments.next() {
        if argument == flag {
            return arguments.next();
        }
    }
    None
}
