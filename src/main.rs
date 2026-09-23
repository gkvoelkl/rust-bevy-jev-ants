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
            window_level: bevy::window::WindowLevel::AlwaysOnTop,
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
