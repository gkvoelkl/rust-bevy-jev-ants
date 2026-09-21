//! What a single ant can see.
//!
//! The result is deliberately a handful of sentences, not coordinates. An ant
//! that sees three cells far has no idea where on the board it stands, and the
//! state it sends must not give that away either — otherwise the model decides
//! with knowledge the ant does not have.

use bevy::prelude::*;

use super::grid::{Dir, Grid, Occupancy, Occupant};
use super::nest::Nest;
use super::scent::{self, Scent};

/// Everything within `radius` cells, described relative to the ant: other ants
/// and the edge of the world. Sorted by distance, so two ants in the same
/// situation produce exactly the same text.
pub fn sightings(
    grid: Grid,
    occupancy: &Occupancy,
    nest: Nest,
    scent: &Scent,
    from: IVec2,
    radius: i32,
) -> Vec<String> {
    let mut found: Vec<(i32, Dir, String)> = Vec::new();

    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let offset = IVec2::new(dx, dy);
            if offset == IVec2::ZERO {
                continue;
            }
            let what = match occupancy.at(grid, from + offset) {
                Some(Occupant::Ant(_)) => "another ant",
                Some(Occupant::Fruit(_)) => "a fruit",
                None => continue,
            };
            let distance = dx.abs().max(dy.abs());
            found.push((distance, Dir::nearest(offset), what.to_string()));
        }
    }

    // The edge is only mentioned where it is actually within sight.
    for direction in Dir::COMPASS {
        for distance in 1..=radius {
            if !grid.contains(from + direction.offset() * distance) {
                found.push((distance, direction, "the edge of the world".to_string()));
                break;
            }
        }
    }

    // The nest is seen like anything else: only when it is close enough. An ant
    // that cannot see it does not know where home is — which is precisely the
    // gap the pheromones of step 3 will fill.
    let nest_sighting = if nest.contains(from) {
        Some("the nest, right where you stand".to_string())
    } else {
        let target = nest.nearest_cell(from);
        let offset = target - from;
        let distance = offset.x.abs().max(offset.y.abs());
        (distance <= radius).then(|| {
            let cells = if distance == 1 { "cell" } else { "cells" };
            format!(
                "the nest, {distance} {cells} to the {}",
                Dir::nearest(offset).spoken()
            )
        })
    };

    // Smelling is a close-range sense, not sight: only the cells next door.
    let trail = scent.uphill(grid, from).map(|(direction, strength)| {
        format!(
            "a {} scent trail leading {} — the colony's own, it leads home",
            scent::describe(strength),
            direction.spoken()
        )
    });

    found.sort_by_key(|(distance, direction, _)| (*distance, direction.key()));
    let around = found.into_iter().map(|(distance, direction, what)| {
        let cells = if distance == 1 { "cell" } else { "cells" };
        format!("{what}, {distance} {cells} to the {}", direction.spoken())
    });

    nest_sighting
        .into_iter()
        .chain(trail)
        .chain(around)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default nest sits at the centre of the real board, far outside the
    /// small test grid, so it never shows up unless a test puts it in reach.
    fn far_away_nest() -> Nest {
        Nest::default()
    }

    fn board() -> (Grid, Occupancy) {
        let grid = Grid {
            width: 10,
            height: 10,
        };
        let occupancy = Occupancy::new(grid);
        (grid, occupancy)
    }

    #[test]
    fn an_ant_in_the_open_sees_nothing() {
        let (grid, occupancy) = board();
        assert!(
            sightings(
                grid,
                &occupancy,
                far_away_nest(),
                &Scent::new(grid),
                IVec2::new(5, 5),
                3
            )
            .is_empty()
        );
    }

    #[test]
    fn a_neighbour_is_reported_with_distance_and_direction() {
        let (grid, mut occupancy) = board();
        occupancy.occupy(
            grid,
            IVec2::new(5, 7),
            Occupant::Ant(Entity::from_raw_u32(1).unwrap()),
        );

        let seen = sightings(
            grid,
            &occupancy,
            far_away_nest(),
            &Scent::new(grid),
            IVec2::new(5, 5),
            3,
        );
        assert_eq!(seen, vec!["another ant, 2 cells to the north"]);
    }

    /// A fruit must never be described as an ant — the state would be a lie and
    /// the decision would rest on it.
    #[test]
    fn a_fruit_is_told_apart_from_an_ant() {
        let (grid, mut occupancy) = board();
        occupancy.occupy(
            grid,
            IVec2::new(7, 5),
            Occupant::Fruit(Entity::from_raw_u32(9).unwrap()),
        );

        let seen = sightings(
            grid,
            &occupancy,
            far_away_nest(),
            &Scent::new(grid),
            IVec2::new(5, 5),
            3,
        );
        assert_eq!(seen, vec!["a fruit, 2 cells to the east"]);
    }

    #[test]
    fn an_ant_beyond_the_radius_is_invisible() {
        let (grid, mut occupancy) = board();
        occupancy.occupy(
            grid,
            IVec2::new(9, 5),
            Occupant::Ant(Entity::from_raw_u32(1).unwrap()),
        );

        assert!(
            sightings(
                grid,
                &occupancy,
                far_away_nest(),
                &Scent::new(grid),
                IVec2::new(5, 5),
                3
            )
            .is_empty()
        );
    }

    #[test]
    fn the_edge_is_seen_only_when_it_is_close() {
        let (grid, occupancy) = board();

        // The distance is the first cell the ant cannot enter.
        let in_the_corner = sightings(
            grid,
            &occupancy,
            far_away_nest(),
            &Scent::new(grid),
            IVec2::new(0, 0),
            3,
        );
        assert!(in_the_corner.contains(&"the edge of the world, 1 cell to the south".to_string()));
        assert!(in_the_corner.contains(&"the edge of the world, 1 cell to the west".to_string()));

        let one_cell_in = sightings(
            grid,
            &occupancy,
            far_away_nest(),
            &Scent::new(grid),
            IVec2::new(1, 1),
            3,
        );
        assert!(one_cell_in.contains(&"the edge of the world, 2 cells to the south".to_string()));

        assert!(
            sightings(
                grid,
                &occupancy,
                far_away_nest(),
                &Scent::new(grid),
                IVec2::new(5, 5),
                3
            )
            .is_empty()
        );
    }

    /// Two ants in the same situation must produce byte-identical text — that is
    /// what makes decisions comparable later.
    #[test]
    fn the_same_situation_produces_the_same_text() {
        let (grid, mut occupancy) = board();
        occupancy.occupy(
            grid,
            IVec2::new(4, 4),
            Occupant::Ant(Entity::from_raw_u32(1).unwrap()),
        );
        occupancy.occupy(
            grid,
            IVec2::new(7, 7),
            Occupant::Ant(Entity::from_raw_u32(2).unwrap()),
        );

        let one = sightings(
            grid,
            &occupancy,
            far_away_nest(),
            &Scent::new(grid),
            IVec2::new(5, 5),
            1,
        );
        let other = sightings(
            grid,
            &occupancy,
            far_away_nest(),
            &Scent::new(grid),
            IVec2::new(8, 8),
            1,
        );
        assert_eq!(one, other);
        assert_eq!(one, vec!["another ant, 1 cell to the south-west"]);
    }
}
