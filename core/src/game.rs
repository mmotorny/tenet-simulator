// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Game state and movement rules. No time mechanics yet: one move is one turn.

use crate::level::{Level, Pos};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    North,
    South,
    East,
    West,
}

impl Dir {
    fn offset(self, pos: Pos) -> Pos {
        match self {
            Dir::North => Pos::new(pos.x, pos.y - 1),
            Dir::South => Pos::new(pos.x, pos.y + 1),
            Dir::East => Pos::new(pos.x + 1, pos.y),
            Dir::West => Pos::new(pos.x - 1, pos.y),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Moved,
    /// The move was not possible; no turn passes.
    Blocked,
    /// The protagonist reached the exit door.
    Exited,
}

#[derive(Clone, Debug)]
pub struct Game {
    level: Level,
    player: Pos,
    turn: u32,
    status: Status,
}

impl Game {
    /// Starts a level with the protagonist standing in the entry door.
    pub fn new(level: Level) -> Self {
        let player = level.entry();
        Self { level, player, turn: 0, status: Status::Playing }
    }

    pub fn level(&self) -> &Level {
        &self.level
    }

    pub fn player(&self) -> Pos {
        self.player
    }

    pub fn turn(&self) -> u32 {
        self.turn
    }

    pub fn status(&self) -> Status {
        self.status
    }

    pub fn step(&mut self, dir: Dir) -> StepOutcome {
        if self.status == Status::Exited {
            return StepOutcome::Blocked;
        }
        let target = dir.offset(self.player);
        if !self.level.tile(target).is_some_and(|t| t.is_walkable()) {
            return StepOutcome::Blocked;
        }
        self.player = target;
        self.turn += 1;
        if target == self.level.exit() {
            self.status = Status::Exited;
            StepOutcome::Exited
        } else {
            StepOutcome::Moved
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Dir::*;

    fn game(map: &str) -> Game {
        Game::new(Level::parse(map).unwrap())
    }

    #[test]
    fn starts_in_the_entry_door() {
        let g = game("WWWW\nE..X\nWWWW");
        assert_eq!(g.player(), Pos::new(0, 1));
        assert_eq!((g.turn(), g.status()), (0, Status::Playing));
    }

    #[test]
    fn walls_glass_and_turnstile_block_without_spending_a_turn() {
        let mut g = game("WWWWW\nE.g.X\nWTrbW\nWWWWW");
        assert_eq!(g.step(West), StepOutcome::Blocked); // off the map through the entry door
        assert_eq!(g.step(North), StepOutcome::Blocked);
        assert_eq!(g.step(East), StepOutcome::Moved);
        assert_eq!(g.step(East), StepOutcome::Blocked); // glass
        assert_eq!(g.step(South), StepOutcome::Blocked); // turnstile frame
        assert_eq!(g.turn(), 1);
    }

    #[test]
    fn turnstile_is_a_passage_through_the_glass() {
        let level = Level::parse(crate::levels::FREEPORT).unwrap();
        let mut g = Game::new(level);
        let route = "ddddwwdddddddddwwd";
        for (i, ch) in route.chars().enumerate() {
            let dir = match ch {
                'w' => North,
                'd' => East,
                _ => unreachable!(),
            };
            let expected = if i + 1 == route.len() { StepOutcome::Exited } else { StepOutcome::Moved };
            assert_eq!(g.step(dir), expected, "move {i} ({ch})");
        }
        assert_eq!(g.turn(), route.len() as u32);
    }

    #[test]
    fn reaching_the_exit_ends_the_level() {
        let mut g = game("WWWW\nE..X\nWWWW");
        assert_eq!(g.step(East), StepOutcome::Moved);
        assert_eq!(g.step(East), StepOutcome::Moved);
        assert_eq!(g.step(East), StepOutcome::Exited);
        assert_eq!(g.status(), Status::Exited);
        assert_eq!(g.step(West), StepOutcome::Blocked);
        assert_eq!(g.turn(), 3);
    }
}
