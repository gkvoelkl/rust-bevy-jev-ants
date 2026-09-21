mod ants;
mod api;
mod config;
mod decisions;
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

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Ant Colony — Step 1".into(),
                resolution: (960u32, 780u32).into(),
                ..default()
            }),
            ..default()
        }))
        // An order given on the command line counts as already said: it is in
        // effect and shows up in the list, and the field starts empty.
        .insert_resource(OrderHistory::starting_with(&first_order))
        .insert_resource(QueenOrder(first_order))
        .add_plugins((
            world::WorldPlugin,
            DecisionsPlugin {
                source: DecisionsFrom::Jev,
            },
            ants::AntsPlugin,
            ui::UiPlugin,
        ))
        .run();
}

/// `cargo run -- --order "Geht alle nach Osten"` sets the first order, which is
/// otherwise given through the text field.
fn order_from_args() -> String {
    let mut arguments = std::env::args();
    while let Some(argument) = arguments.next() {
        if argument == "--order" {
            return arguments.next().unwrap_or_default();
        }
    }
    String::new()
}
