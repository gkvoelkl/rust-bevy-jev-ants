//! `cargo run -- --compare`
//!
//! The question 2g exists for: do the pheromones earn their keep? Eyeballing the
//! HUD gave readings between 10 and 18 requests per fruit in the *same*
//! configuration, so the spread was as big as the effect. This runs the colony
//! headless with the clock in hand, several times per configuration, and prints
//! the spread along with the average — a single number would invite the same
//! mistake again.
//!
//! The rule baseline is measured here, not Jev: it needs no key, costs nothing,
//! and answers the question that is about the world rather than about the
//! model. It is a yardstick, never a way to play. `--compare jev` adds the
//! model, in real time and for real money.

use std::time::{Duration, Instant};

use bevy::prelude::*;

use crate::ants::AntsPlugin;
use crate::config::{ANT_COUNT, FRUIT_TARGET, THINK_INTERVAL};
use crate::decisions::{DecisionStats, DecisionsFrom, DecisionsPlugin, QueenOrder};
use crate::world::WorldPlugin;
use crate::world::grid::Grid;
use crate::world::nest::Stores;
use crate::world::scent::Scent;

/// Simulated seconds per run.
const RUN_SECONDS: f32 = 120.0;
/// How many runs per configuration. Enough to show the spread.
const REPEATS: usize = 5;
const STEP: Duration = Duration::from_millis(50);

struct Outcome {
    stored: u32,
    requests: u32,
}

pub fn run(with_jev: bool) {
    println!(
        "{ANT_COUNT} ants, {FRUIT_TARGET} fruits, thinking every {THINK_INTERVAL:.0} s, \
         {RUN_SECONDS:.0} simulated seconds per run\n"
    );

    for smelling in [false, true] {
        let mut outcomes = Vec::new();
        for _ in 0..REPEATS {
            outcomes.push(simulate(smelling, "Bringt die Früchte heim"));
        }
        report(
            if smelling {
                "baseline rules, with pheromones"
            } else {
                "baseline rules, no pheromones"
            },
            &outcomes,
        );
    }

    if with_jev {
        println!("\nNow the same with Jev, in real time — this one costs money.");
        for smelling in [false, true] {
            let outcome = live(smelling);
            report(
                if smelling {
                    "Jev, with pheromones"
                } else {
                    "Jev, no pheromones"
                },
                &[outcome],
            );
        }
    }
}

/// One headless run with the clock set by hand, so the result does not depend on
/// how fast this machine happens to be.
fn simulate(smelling: bool, order: &str) -> Outcome {
    let mut app = App::new();
    app.insert_resource(Time::<()>::default());
    app.add_plugins((
        WorldPlugin,
        DecisionsPlugin {
            source: DecisionsFrom::RulesForTesting,
        },
        AntsPlugin,
    ));
    app.insert_resource(QueenOrder(order.to_string()));
    app.finish();
    app.cleanup();
    app.update();

    if !smelling {
        let grid = *app.world().resource::<Grid>();
        app.insert_resource(Scent::without_any(grid));
    }

    let steps = (RUN_SECONDS / STEP.as_secs_f32()) as u32;
    for _ in 0..steps {
        app.world_mut().resource_mut::<Time>().advance_by(STEP);
        app.update();
    }

    Outcome {
        stored: app.world().resource::<Stores>().0,
        requests: {
            let stats = app.world().resource::<DecisionStats>();
            stats.from_jev + stats.from_rules
        },
    }
}

/// One run against the live API. The clock has to be the real one — an answer
/// takes some 700 ms whatever the simulation believes about time.
fn live(smelling: bool) -> Outcome {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_plugins((
        WorldPlugin,
        DecisionsPlugin {
            source: DecisionsFrom::Jev,
        },
        AntsPlugin,
    ));
    app.insert_resource(QueenOrder("Bringt die Früchte heim".to_string()));
    app.finish();
    app.cleanup();
    app.update();

    if !smelling {
        let grid = *app.world().resource::<Grid>();
        app.insert_resource(Scent::without_any(grid));
    }

    let started = Instant::now();
    while started.elapsed().as_secs_f32() < RUN_SECONDS {
        app.update();
    }

    Outcome {
        stored: app.world().resource::<Stores>().0,
        requests: app.world().resource::<DecisionStats>().from_jev,
    }
}

fn report(what: &str, outcomes: &[Outcome]) {
    let stored: Vec<u32> = outcomes.iter().map(|outcome| outcome.stored).collect();
    let requests: u32 = outcomes.iter().map(|outcome| outcome.requests).sum();
    let total: u32 = stored.iter().sum();

    let per_minute =
        f64::from(total) / outcomes.len() as f64 / f64::from(RUN_SECONDS as u32) * 60.0;
    let per_fruit = if total > 0 {
        format!("{:.1}", f64::from(requests) / f64::from(total))
    } else {
        "—".to_string()
    };

    println!(
        "{what:<32} {per_minute:5.1} fruits/min   {per_fruit:>5} requests/fruit   \
         runs: {stored:?}"
    );
}
