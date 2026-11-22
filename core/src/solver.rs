// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Shortest solutions: a level's par (the fewest moves that finish it), and the predicted
//! completion of a game in progress (the rest of the history the universe would have).
//!
//! Plain breadth-first pathfinding over (position, global time, arrow).

use std::collections::{HashSet, VecDeque};

use crate::game::{Arrow, Dir, Game, Status, StepOutcome};
use crate::level::{Level, Pos};


/// A search state: where the protagonist is, when, and which way their time runs.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Place {
    player: Pos,
    now: i32,
    arrow: Arrow,
}

/// The moves of a shortest solution, or `None` if the level can't be finished within `max_moves`.
pub fn shortest_solution(level: &Level, max_moves: usize) -> Option<Vec<Dir>> {
    completion(&Game::new(level.clone()), max_moves)
}

/// The fewest further moves that finish `game`: plain pathfinding over (position, global time,
/// arrow), visiting each once, with every move checked by the game's own rules.
pub fn completion(game: &Game, max_moves: usize) -> Option<Vec<Dir>> {
    if game.status() == Status::Exited {
        return Some(Vec::new());
    }
    // Global time the search may explore: around the level's own window and the current time,
    // widened by the number of cells, so no path is cut short by the bounds (which only keep the
    // search finite).
    let level = game.level();
    let cells = level.width() * level.height();
    let (lo, hi) = level.timeline();
    let window = (lo.min(game.now()) - cells)..=(hi.max(game.now()) + cells);
    let place = |g: &Game| Place { player: g.player(), now: g.now(), arrow: g.arrow() };
    let mut seen = HashSet::from([place(game)]);
    let mut queue = VecDeque::from([(game.clone(), Vec::new())]);
    while let Some((state, moves)) = queue.pop_front() {
        if moves.len() >= max_moves {
            continue;
        }
        for dir in [Dir::North, Dir::South, Dir::East, Dir::West] {
            let mut next = state.clone();
            if next.step(dir) == StepOutcome::Blocked || !window.contains(&next.now()) {
                continue;
            }
            let mut path = moves.clone();
            path.push(dir);
            if next.status() == Status::Exited {
                return Some(path);
            }
            if seen.insert(place(&next)) {
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
        assert_eq!(completion(&Game::new(level), 20), None);
    }
}

