//! A hand-built board with a task, as opposed to the endless field.
//!
//! The sandbox has no goal, so there is nothing to solve and nothing to get
//! wrong — and a colony you cannot fail at is a colony you stop talking to. A
//! scenario gives three things the sandbox lacks: a fixed board, a condition to
//! meet, and the chance to try the sentence again.
//!
//! The board lives in `assets/scenarios/`, beside `questions.ron`, for the same
//! reason: designing a task is writing, not programming.

use bevy::prelude::*;
use serde::Deserialize;

use super::grid::Grid;
use super::nest::Nest;

/// Where the boards live, relative to the working directory.
pub const SCENARIO_DIR: &str = "assets/scenarios";

/// The levels, in the order they are meant to be met. A list rather than a
/// directory listing, because the order is part of the design: the first board
/// teaches the loop, the second asks something of it.
pub const LEVELS: [&str; 2] = ["fruit", "plank"];

/// One task. Everything the sandbox would take from `config.rs` comes from here
/// instead, so a board can be small and crowded where the open field is wide.
#[derive(Resource, Clone, Deserialize)]
pub struct Scenario {
    /// What the level is called, for the picker.
    pub name: String,
    /// Shown to the player. The ants never read it — they know only what they
    /// see, and telling them the plan would be the end of the puzzle.
    pub briefing: String,
    pub width: i32,
    pub height: i32,
    /// Bottom-left corner of the nest, and its edge length.
    pub nest_at: (i32, i32),
    pub nest_size: i32,
    pub ants: usize,
    /// Cells the river runs through. One wide is enough to stop a colony.
    #[serde(default)]
    pub water: Vec<(i32, i32)>,
    #[serde(default)]
    pub fruit: Vec<(i32, i32)>,
    /// Where the plank starts, if the board has one.
    #[serde(default)]
    pub plank_at: Option<(i32, i32)>,
    /// How many fruits have to reach the stores for the task to be solved.
    pub target: u32,
}

impl Scenario {
    pub fn grid(&self) -> Grid {
        Grid {
            width: self.width,
            height: self.height,
        }
    }

    pub fn nest(&self) -> Nest {
        Nest {
            min: IVec2::new(self.nest_at.0, self.nest_at.1),
            size: self.nest_size,
        }
    }

    /// Reads one board by name, or says why it could not. A bad file is never
    /// a crash: the game says so and falls back to the open field.
    pub fn load(name: &str) -> Result<Self, String> {
        let path = format!("{SCENARIO_DIR}/{name}.ron");

        #[cfg(target_arch = "wasm32")]
        {
            let _ = path;
            Err("no file system in the browser".to_string())
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let text =
                std::fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
            let scenario: Scenario =
                ron::from_str(&text).map_err(|error| format!("{path}: {error}"))?;
            scenario.check().map_err(|why| format!("{path}: {why}"))?;
            Ok(scenario)
        }
    }

    /// Every level that reads. A broken board is left out with a line in the
    /// log rather than taking the game down with it.
    pub fn all() -> Vec<Scenario> {
        LEVELS
            .iter()
            .filter_map(|name| match Scenario::load(name) {
                Ok(scenario) => Some(scenario),
                Err(reason) => {
                    warn!("level '{name}' is unplayable — {reason}");
                    None
                }
            })
            .collect()
    }

    /// Catches the mistakes that make a board unplayable before the player
    /// meets them — a fruit in the river, a nest off the edge, a task nobody
    /// could finish.
    fn check(&self) -> Result<(), String> {
        let grid = self.grid();
        let nest = self.nest();

        for cell in nest.cells() {
            if !grid.contains(cell) {
                return Err(format!("the nest reaches off the board at {cell:?}"));
            }
        }
        for (label, cells) in [("water", &self.water), ("fruit", &self.fruit)] {
            for (x, y) in cells {
                let cell = IVec2::new(*x, *y);
                if !grid.contains(cell) {
                    return Err(format!("{label} at {cell:?} is off the board"));
                }
            }
        }
        if let Some((x, y)) = self.plank_at {
            let cell = IVec2::new(x, y);
            if !grid.contains(cell) {
                return Err(format!("the plank at {cell:?} is off the board"));
            }
            if self.water.contains(&(x, y)) {
                return Err("the plank starts in the water".to_string());
            }
        }
        for (x, y) in &self.fruit {
            if self.water.contains(&(*x, *y)) {
                return Err(format!("a fruit floats in the river at ({x}, {y})"));
            }
        }
        if self.target as usize > self.fruit.len() {
            return Err(format!(
                "{} fruits to fetch but only {} on the board",
                self.target,
                self.fruit.len()
            ));
        }
        if self.ants == 0 {
            return Err("a colony of no ants".to_string());
        }
        if self.name.trim().is_empty() {
            return Err("a level with no name cannot be offered".to_string());
        }
        // Eight ants in nine cells spend their first seconds untangling
        // themselves instead of answering.
        if self.ants > (self.nest_size * self.nest_size) as usize {
            return Err(format!(
                "{} ants will not fit in a nest of {} cells",
                self.ants,
                self.nest_size * self.nest_size
            ));
        }
        Ok(())
    }
}

/// Whether the task is done. Kept apart from `Stores` so the banner can stay up
/// while the colony carries on.
#[derive(Resource, Default)]
pub struct Solved(pub bool);

pub fn check_solved(
    scenario: Option<Res<Scenario>>,
    stores: Res<super::nest::Stores>,
    mut solved: ResMut<Solved>,
) {
    let Some(scenario) = scenario else {
        return;
    };
    if !solved.0 && stores.0 >= scenario.target {
        info!("the task is solved: {} fruits home", stores.0);
        solved.0 = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plank_board() -> Scenario {
        Scenario::load("plank").expect("assets/scenarios/plank.ron parses")
    }

    /// The board that ships has to load, or the game quietly falls back to the
    /// open field and the player wonders where the river went.
    #[test]
    fn the_shipped_board_reads() {
        let scenario = plank_board();
        assert!(scenario.target > 0);
        assert!(scenario.plank_at.is_some(), "this one is about the plank");
    }

    /// Every level that ships has to load, or it silently vanishes from the
    /// picker and nobody finds out why.
    #[test]
    fn every_level_reads() {
        let levels = Scenario::all();
        assert_eq!(levels.len(), LEVELS.len(), "a level failed to load");
        for level in &levels {
            assert!(!level.name.trim().is_empty());
            assert!(!level.briefing.trim().is_empty());
        }
    }

    /// The point of the board: the fruit is out of reach until the river is
    /// bridged. If a way round exists, there is no puzzle.
    #[test]
    fn the_fruit_is_walled_off_by_the_river() {
        let scenario = plank_board();
        let grid = scenario.grid();
        let water: std::collections::HashSet<(i32, i32)> = scenario.water.iter().copied().collect();

        // Flood fill from the nest across everything that is not water.
        let mut seen = std::collections::HashSet::new();
        let mut queue: Vec<IVec2> = scenario.nest().cells().collect();
        seen.extend(queue.iter().copied());
        while let Some(cell) = queue.pop() {
            for direction in crate::world::grid::Dir::COMPASS {
                let next = cell + direction.offset();
                if grid.contains(next) && !water.contains(&(next.x, next.y)) && seen.insert(next) {
                    queue.push(next);
                }
            }
        }

        for (x, y) in &scenario.fruit {
            assert!(
                !seen.contains(&IVec2::new(*x, *y)),
                "the fruit at ({x}, {y}) can be reached without the plank"
            );
        }
        let (px, py) = scenario.plank_at.expect("there is a plank");
        assert!(
            seen.contains(&IVec2::new(px, py)),
            "the plank itself has to be reachable, or the task is impossible"
        );
    }

    #[test]
    fn a_board_asking_for_more_fruit_than_it_has_is_refused() {
        let mut scenario = plank_board();
        scenario.target = scenario.fruit.len() as u32 + 1;
        assert!(scenario.check().is_err());
    }
}
