// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Shortest solutions, for a level's par (the fewest moves that finish it).
//!
//! A breadth-first search over game states. Two states are the same if the protagonist is in the
//! same place, at the same global time, with the same arrow, and earlier selves stand on the
//! turnstile's trapdoors at the same times (the only way the past affects what's possible).

use std::collections::{HashSet, VecDeque};

use crate::game::{Arrow, Dir, Game, Status, StepOutcome};
use crate::level::{Level, Pos};

/// How far from global time 0 the search explores; beyond it, nothing new can happen in a level
/// whose events all sit near the start.
const TIME_LIMIT: i32 = 64;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    player: Pos,
    now: i32,
    arrow: Arrow,
    trapdoors: Vec<(Pos, i32)>,
}

fn key(game: &Game) -> Key {
    let trapdoors = game.level().turnstile().map_or_else(Vec::new, |t| {
        let mut occupied: Vec<(Pos, i32)> = game
            .segments()
            .iter()
            .flat_map(|s| s.path.iter().enumerate().map(move |(i, &p)| (p, s.start + i as i32 * s.arrow.dt())))
            .filter(|&(p, _)| p == t.red_cell || p == t.blue_cell)
            .collect();
        occupied.sort_by_key(|&(p, t)| (p.x, p.y, t));
        occupied
    });
    Key { player: game.player(), now: game.now(), arrow: game.arrow(), trapdoors }
}

/// The moves of a shortest solution, or `None` if the level can't be finished within `max_moves`.
pub fn shortest_solution(level: &Level, max_moves: usize) -> Option<Vec<Dir>> {
    let start = Game::new(level.clone());
    let mut seen = HashSet::from([key(&start)]);
    let mut queue = VecDeque::from([(start, Vec::new())]);
    while let Some((game, moves)) = queue.pop_front() {
        if moves.len() >= max_moves {
            continue;
        }
        for dir in [Dir::North, Dir::South, Dir::East, Dir::West] {
            let mut next = game.clone();
            let outcome = next.step(dir);
            if outcome == StepOutcome::Blocked || next.now().abs() > TIME_LIMIT {
                continue;
            }
            let mut path = moves.clone();
            path.push(dir);
            if next.status() == Status::Exited {
                return Some(path);
            }
            if seen.insert(key(&next)) {
                queue.push_back((next, path));
            }
        }
    }
    None
}

/// The fewest moves that finish `level`, if it can be finished within `max_moves`.
pub fn par(level: &Level, max_moves: usize) -> Option<u32> {
    shortest_solution(level, max_moves).map(|moves| moves.len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_shortest_walk() {
        let level = Level::parse("SE..XF").unwrap();
        assert_eq!(par(&level, 10), Some(5));
    }

    #[test]
    fn needs_the_turnstile_when_the_exit_closes_too_early() {
        // The exit closes at t = 3: walking straight there arrives at t = 4, too late.
        let level = Level::parse("SEr____b..XF\n\nexit-closes: 3").unwrap();
        let moves = shortest_solution(&level, 30).unwrap();
        let mut game = Game::new(level);
        for dir in &moves {
            game.step(*dir);
        }
        assert_eq!(game.status(), Status::Exited);
        assert!(game.segments().len() > 1, "the solution inverts");
    }

    #[test]
    fn first_room_par() {
        let level = Level::parse(crate::levels::FREEPORT).unwrap();
        assert_eq!(par(&level, 60), Some(19));
    }

    #[test]
    fn reports_unsolvable_levels() {
        let level = Level::parse("SE.WXF").unwrap();
        assert_eq!(par(&level, 20), None);
    }
}

