//! The scent trail: how an ant finds its way back to the nest.
//!
//! An ant leaves the nest to search, and while it searches it marks the ground
//! behind it — **less with every step since it was last home**. The trail is
//! therefore strongest at the nest and faints towards the outside, so an ant
//! that walks uphill walks home. Without that decay a trail would be a line
//! with no direction, and nobody could tell which end was the nest.
//!
//! Only searching ants mark. A carrier has its hands full and follows what was
//! laid on the way out — usually its own trail, which is exactly how real ants
//! do it.
//!
//! One field, then, and one purpose. Nothing here helps find food; finding food
//! is what eyes are for.

use bevy::prelude::*;

use crate::config::{SCENT_DEPOSIT, SCENT_HALF_LIFE, SCENT_STEP_DECAY, SCENT_THRESHOLD};

use super::grid::{Dir, Grid};

/// One value per cell, laid out like `Occupancy`.
#[derive(Resource)]
pub struct Scent {
    cells: Vec<f32>,
    /// Switched off for the comparison runs, so a colony without any sense of
    /// smell can be measured against one with it.
    laying: bool,
}

impl Scent {
    pub fn new(grid: Grid) -> Self {
        Self {
            cells: vec![0.0; (grid.width * grid.height) as usize],
            laying: true,
        }
    }

    /// A colony with no sense of smell: nothing is ever marked, so nothing can
    /// ever be followed.
    pub fn without_any(grid: Grid) -> Self {
        Self {
            laying: false,
            ..Self::new(grid)
        }
    }

    /// How much is on this cell. Off the board reads as nothing.
    pub fn at(&self, grid: Grid, cell: IVec2) -> f32 {
        if !grid.contains(cell) {
            return 0.0;
        }
        self.cells[index(grid, cell)]
    }

    /// What a searching ant leaves behind, `steps` steps away from the nest.
    pub fn deposit(&mut self, grid: Grid, cell: IVec2, steps: u32) {
        if !self.laying || !grid.contains(cell) {
            return;
        }
        let amount = SCENT_DEPOSIT * SCENT_STEP_DECAY.powi(steps as i32);
        let slot = &mut self.cells[index(grid, cell)];

        // The strongest deposit wins; they do **not** add up. Adding them looks
        // right and is wrong: a hundred faint traces far from the nest would
        // then outweigh one strong one close to it, and the slope would point
        // away from home instead of towards it. Traffic still counts — a path
        // walked often is refreshed often and so outlives evaporation.
        *slot = slot.max(amount);
    }

    /// The neighbouring cell that smells strongest — if it smells at all and
    /// smells stronger than where the ant stands. Uphill is the way home.
    ///
    /// Standing on the peak gives `None`. That is not a failure: the peak is at
    /// the nest, and an ant standing there has arrived.
    pub fn uphill(&self, grid: Grid, from: IVec2) -> Option<(Dir, f32)> {
        let here = self.at(grid, from);
        Dir::COMPASS
            .into_iter()
            .map(|direction| (direction, self.at(grid, from + direction.offset())))
            .filter(|(_, strength)| *strength >= SCENT_THRESHOLD && *strength > here)
            .max_by(|one, other| one.1.total_cmp(&other.1))
    }

    /// Time passing. A trail halves every `SCENT_HALF_LIFE` seconds, and what
    /// falls under the threshold is cleared so stale trails really disappear.
    pub fn evaporate(&mut self, seconds: f32) {
        let factor = 0.5f32.powf(seconds / SCENT_HALF_LIFE);
        for cell in &mut self.cells {
            *cell *= factor;
            if *cell < SCENT_THRESHOLD {
                *cell = 0.0;
            }
        }
    }
}

fn index(grid: Grid, cell: IVec2) -> usize {
    (cell.y * grid.width + cell.x) as usize
}

/// How a trail is put to an ant: a word, never a number. A number would promise
/// a precision the field does not have, and the model reads words better.
pub fn describe(strength: f32) -> &'static str {
    if strength >= 0.7 {
        "strong"
    } else if strength >= 0.3 {
        "clear"
    } else {
        "faint"
    }
}

pub fn evaporate(time: Res<Time>, mut scent: ResMut<Scent>) {
    scent.evaporate(time.delta_secs());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board() -> (Grid, Scent) {
        let grid = Grid {
            width: 10,
            height: 10,
        };
        (grid, Scent::new(grid))
    }

    #[test]
    fn a_fresh_carrier_leaves_the_most() {
        let (grid, mut scent) = board();
        scent.deposit(grid, IVec2::new(2, 2), 0);
        assert!((scent.at(grid, IVec2::new(2, 2)) - SCENT_DEPOSIT).abs() < 1e-6);
    }

    /// The point of the whole thing: the trail points back to the nest.
    #[test]
    fn the_trail_gets_fainter_the_further_from_the_nest() {
        let (grid, mut scent) = board();
        for step in 0..5u32 {
            scent.deposit(grid, IVec2::new(2 + step as i32, 2), step);
        }

        let strengths: Vec<f32> = (0..5)
            .map(|step| scent.at(grid, IVec2::new(2 + step, 2)))
            .collect();
        for pair in strengths.windows(2) {
            assert!(
                pair[0] > pair[1],
                "scent must fall along the way: {strengths:?}"
            );
        }
    }

    /// The mistake this guards against: deposits adding up. Plenty of ants pass
    /// through a spot far from the nest; if their faint traces summed, that spot
    /// would outsmell the nest itself and the slope would lead away from home.
    #[test]
    fn many_faint_traces_never_beat_one_strong_one() {
        let (grid, mut scent) = board();
        let far_out = IVec2::new(2, 2);
        let next_to_the_nest = IVec2::new(5, 5);

        for _ in 0..50 {
            scent.deposit(grid, far_out, 20); // twenty steps from home
        }
        scent.deposit(grid, next_to_the_nest, 0); // just left the nest

        assert!(
            scent.at(grid, next_to_the_nest) > scent.at(grid, far_out),
            "home must smell stronger than a well-trodden spot far away"
        );
    }

    /// Traffic still matters, only differently: a path walked again is refreshed
    /// and outlives evaporation.
    #[test]
    fn walking_again_refreshes_a_fading_trail() {
        let (grid, mut scent) = board();
        let cell = IVec2::new(2, 2);

        scent.deposit(grid, cell, 0);
        scent.evaporate(SCENT_HALF_LIFE);
        let faded = scent.at(grid, cell);

        scent.deposit(grid, cell, 0);
        assert!(scent.at(grid, cell) > faded);
    }

    #[test]
    fn a_trail_nobody_refreshes_disappears() {
        let (grid, mut scent) = board();
        scent.deposit(grid, IVec2::new(2, 2), 0);

        // Well past several half-lives.
        scent.evaporate(SCENT_HALF_LIFE * 8.0);
        assert_eq!(
            scent.at(grid, IVec2::new(2, 2)),
            0.0,
            "what falls under the threshold is cleared, not left as dust"
        );
    }

    #[test]
    fn uphill_points_at_the_strongest_neighbour() {
        let (grid, mut scent) = board();
        let here = IVec2::new(5, 5);
        scent.deposit(grid, here + IVec2::new(1, 0), 6); // east, weaker
        scent.deposit(grid, here + IVec2::new(0, 1), 0); // north, freshest

        let (direction, _) = scent.uphill(grid, here).expect("a way uphill");
        assert_eq!(direction, Dir::North);
    }

    /// On the peak there is nothing further up — and the peak is where the
    /// fruit was.
    #[test]
    fn on_the_peak_there_is_no_way_up() {
        let (grid, mut scent) = board();
        let here = IVec2::new(5, 5);
        scent.deposit(grid, here, 0);
        scent.deposit(grid, here + IVec2::new(1, 0), 6);

        assert!(scent.uphill(grid, here).is_none());
    }

    #[test]
    fn a_trail_under_the_threshold_is_not_a_trail() {
        let (grid, mut scent) = board();
        let here = IVec2::new(5, 5);
        // Deposited long ago and nearly faded.
        scent.deposit(grid, here + IVec2::new(1, 0), 0);
        scent.evaporate(SCENT_HALF_LIFE * 5.0);

        assert!(scent.uphill(grid, here).is_none());
    }

    #[test]
    fn strength_is_told_as_a_word() {
        assert_eq!(describe(0.9), "strong");
        assert_eq!(describe(0.5), "clear");
        assert_eq!(describe(0.1), "faint");
    }

    #[test]
    fn a_colony_without_smell_leaves_nothing() {
        let grid = Grid {
            width: 10,
            height: 10,
        };
        let mut scent = Scent::without_any(grid);

        scent.deposit(grid, IVec2::new(2, 2), 0);
        assert_eq!(scent.at(grid, IVec2::new(2, 2)), 0.0);
        assert!(scent.uphill(grid, IVec2::new(2, 3)).is_none());
    }

    #[test]
    fn off_the_board_is_simply_nothing() {
        let (grid, mut scent) = board();
        scent.deposit(grid, IVec2::new(-1, 4), 0);
        assert_eq!(scent.at(grid, IVec2::new(-1, 4)), 0.0);
        assert_eq!(scent.at(grid, IVec2::new(99, 99)), 0.0);
    }
}
