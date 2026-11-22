// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Shortest solutions: a level's par (the fewest moves that finish it), and the predicted
//! completion of a game in progress (the rest of the history the universe would have).
//!
//! Plain breadth-first pathfinding over (position, global time, arrow), up to a move budget: it
//! doesn't prove that no longer solution exists. Among equally short
//! completions it can prefer ones that put new selves at given places at the current time (the
//! session uses this to make your other selves mirror you).

use std::collections::HashMap;

use crate::game::{Dir, Game, Place, Status, rule};
use crate::level::{Level, Pos};

/// A searched place and the best of the shortest ways there. Small and `Copy`: the way is kept as
/// a parent link, and the only part of the predicted history the rules ever read (who stands on
/// a trapdoor) as a link to the latest node on one.
#[derive(Clone, Copy)]
struct Node {
    place: Place,
    parent: u32,
    dir: Dir,
    /// Total distance from where the selves arriving at the current time are wanted.
    missed: u32,
    /// The latest node on the way here (this one included) that stands on a trapdoor.
    trapdoor: u32,
}

const NONE: u32 = u32::MAX;

/// The moves of a shortest solution, or `None` if the level can't be finished within `max_moves`.
pub fn shortest_solution(level: &Level, max_moves: usize) -> Option<Vec<Dir>> {
    completion(&Game::new(level.clone()), max_moves)
}

/// The fewest further moves that finish `game`: plain pathfinding over (position, global time,
/// arrow), visiting each once, with every move checked by the game's own rules.
pub fn completion(game: &Game, max_moves: usize) -> Option<Vec<Dir>> {
    completion_toward(game, max_moves, &[])
}

/// Like [`completion`], but among the shortest completions prefers the one whose new selves
/// arrive at the current time nearest to `wanted` (by walking distance on the grid).
pub fn completion_toward(game: &Game, max_moves: usize, wanted: &[Pos]) -> Option<Vec<Dir>> {
    if game.status() == Status::Exited {
        return Some(Vec::new());
    }
    let level = game.level();
    let now = game.now();
    // How far a self arriving at `pos` at the current time is from where it is wanted.
    let miss = |pos: Pos| {
        wanted.iter().map(|w| (w.x - pos.x).unsigned_abs() + (w.y - pos.y).unsigned_abs()).min().unwrap_or(0)
    };
    let trapdoors = level.turnstile().map(|t| [t.red_cell, t.blue_cell]);
    let on_trapdoor = |pos: Pos| trapdoors.is_some_and(|cells| cells.contains(&pos));

    let root = Node { place: game.place(), parent: NONE, dir: Dir::North, missed: 0, trapdoor: NONE };
    let mut nodes = vec![root];
    let mut index = HashMap::from([(root.place, 0u32)]);
    // Layer by layer, so every place keeps the best of its shortest ways in.
    let mut layer = 0..1;
    for _ in 0..max_moves {
        let next_layer = nodes.len();
        let mut finished: Option<(u32, Dir, u32)> = None;
        for i in layer.clone() {
            let node = nodes[i];
            // This way's own predicted selves on trapdoors (few: the chain skips everything else).
            let mut standing = Vec::new();
            let mut k = node.trapdoor;
            while k != NONE {
                let on = nodes[k as usize];
                standing.push((on.place.player, on.place.now));
                k = if on.parent == NONE { NONE } else { nodes[on.parent as usize].trapdoor };
            }
            // Someone stands on `pos` at `t`: a lived self, or one predicted on the way here.
            let occupied = |pos: Pos, t: i32| game.occupied(pos, t) || standing.contains(&(pos, t));
            for dir in [Dir::North, Dir::South, Dir::East, Dir::West] {
                let Some(place) = rule(level, node.place, dir, occupied) else { continue };
                // Only a step through time arrives somewhere new at the current time (crossing the
                // mirror keeps you where you are in time, as yourself).
                let arrives = place.now == now && node.place.now != now;
                let missed = node.missed + if arrives { miss(place.player) } else { 0 };
                if place.player == level.finish() {
                    if finished.is_none_or(|(_, _, best)| missed < best) {
                        finished = Some((i as u32, dir, missed));
                    }
                    continue;
                }
                let slot = match index.get(&place) {
                    Some(&j) if (j as usize) < next_layer => continue, // reached sooner already
                    Some(&j) if missed < nodes[j as usize].missed => j,
                    Some(_) => continue,
                    None => {
                        let j = nodes.len() as u32;
                        index.insert(place, j);
                        nodes.push(root);
                        j
                    }
                };
                let trapdoor = if on_trapdoor(place.player) { slot } else { node.trapdoor };
                nodes[slot as usize] = Node { place, parent: i as u32, dir, missed, trapdoor };
            }
        }
        if let Some((parent, dir, _)) = finished {
            let mut moves = vec![dir];
            let mut k = parent;
            while nodes[k as usize].parent != NONE {
                moves.push(nodes[k as usize].dir);
                k = nodes[k as usize].parent;
            }
            moves.reverse();
            return Some(moves);
        }
        if nodes.len() == next_layer {
            return None;
        }
        layer = next_layer..nodes.len();
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
