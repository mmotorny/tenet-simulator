// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Game state and rules: worldlines, arrows of time, and the turnstile.
//!
//! Every move advances the protagonist's own (proper) time by one, and global time by one in the
//! direction of their arrow: +1 forward, −1 inverted. Crossing the turnstile's mirror (between its
//! two interior cells) reverses the arrow and takes no global time: at that instant both selves
//! exist, one on each side of the mirror. The turnstile's doors are closed whenever anyone is
//! inside. The protagonist's history is a worldline made of segments, one per stretch between
//! inversions; earlier segments replay as other selves at whatever global time it is.
//!
//! Consistency is lenient for now: past selves are not obstacles, and what they saw isn't checked.

use crate::level::{Level, Pos, Tile};

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

/// A self's arrow of time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrow {
    Forward,
    Inverted,
}

impl Arrow {
    /// Global time elapsed per move.
    pub fn dt(self) -> i32 {
        match self {
            Arrow::Forward => 1,
            Arrow::Inverted => -1,
        }
    }

    fn reversed(self) -> Self {
        match self {
            Arrow::Forward => Arrow::Inverted,
            Arrow::Inverted => Arrow::Forward,
        }
    }
}

/// One stretch of the worldline between inversions: positions at consecutive moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub arrow: Arrow,
    /// Global time of the first position.
    pub start: i32,
    /// `path[i]` is the position at global time `start + i * arrow.dt()`.
    pub path: Vec<Pos>,
}

impl Segment {
    /// Global time of the last position.
    pub fn end(&self) -> i32 {
        self.start + (self.path.len() as i32 - 1) * self.arrow.dt()
    }

    /// The earliest and latest global time this segment spans.
    pub fn span(&self) -> (i32, i32) {
        (self.start.min(self.end()), self.start.max(self.end()))
    }

    /// Where this self is at global time `t`, if it exists then.
    pub fn pos_at(&self, t: i32) -> Option<Pos> {
        let i = (t - self.start) * self.arrow.dt();
        usize::try_from(i).ok().and_then(|i| self.path.get(i)).copied()
    }
}

/// A self present at some global time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfAt {
    pub pos: Pos,
    pub arrow: Arrow,
    /// Whether this is the self the player is controlling.
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Playing,
    Exited,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Moved,
    /// The move reversed the protagonist's arrow of time.
    Inverted,
    /// The move was not possible; no time passes.
    Blocked,
    /// The protagonist stepped onto the finish and left the level.
    Exited,
}

#[derive(Clone, Debug)]
pub struct Game {
    level: Level,
    segments: Vec<Segment>,
    moves: u32,
    status: Status,
}

impl Game {
    /// Starts a level at global time 0 with the protagonist on the start, outside the entry door.
    pub fn new(level: Level) -> Self {
        let first = Segment { arrow: Arrow::Forward, start: 0, path: vec![level.start()] };
        Self { level, segments: vec![first], moves: 0, status: Status::Playing }
    }

    pub fn level(&self) -> &Level {
        &self.level
    }

    /// The worldline so far, oldest segment first; the last one is the active self.
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    fn active(&self) -> &Segment {
        self.segments.last().expect("a game always has a segment")
    }

    pub fn player(&self) -> Pos {
        *self.active().path.last().expect("a segment always has a position")
    }

    pub fn arrow(&self) -> Arrow {
        self.active().arrow
    }

    /// Current global time.
    pub fn now(&self) -> i32 {
        self.active().end()
    }

    /// Moves made: the protagonist's own elapsed time.
    pub fn moves(&self) -> u32 {
        self.moves
    }

    pub fn status(&self) -> Status {
        self.status
    }

    /// Every self present at global time `t`.
    pub fn selves_at(&self, t: i32) -> Vec<SelfAt> {
        let last = self.segments.len() - 1;
        self.segments
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.pos_at(t).map(|pos| SelfAt { pos, arrow: s.arrow, active: i == last }))
            .collect()
    }

    /// Whether the turnstile's doors are open at global time `t`: they are closed while anyone is
    /// inside.
    pub fn turnstile_open_at(&self, t: i32) -> bool {
        let Some(turnstile) = self.level.turnstile() else { return true };
        !self.segments.iter().filter_map(|s| s.pos_at(t)).any(|pos| turnstile.is_inside(pos))
    }

    pub fn step(&mut self, dir: Dir) -> StepOutcome {
        if self.status == Status::Exited {
            return StepOutcome::Blocked;
        }
        let from = self.player();
        let target = dir.offset(from);
        let crossing = self.level.turnstile().is_some_and(|t| t.crosses_mirror(from, target));
        let arrival = if crossing { self.now() } else { self.now() + self.arrow().dt() };
        let walkable = match self.level.tile(target) {
            Some(Tile::Exit) => self.level.exit_open_at(arrival),
            Some(Tile::TurnstileDoor(_)) => self.turnstile_open_at(arrival),
            Some(tile) => tile.is_walkable(),
            None => false,
        };
        if !walkable {
            return StepOutcome::Blocked;
        }

        self.moves += 1;
        if crossing {
            let arrow = self.arrow().reversed();
            self.segments.push(Segment { arrow, start: arrival, path: vec![target] });
            return StepOutcome::Inverted;
        }
        self.segments.last_mut().expect("a game always has a segment").path.push(target);
        if target == self.level.finish() {
            self.status = Status::Exited;
            return StepOutcome::Exited;
        }
        StepOutcome::Moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Dir::*;

    fn game(map: &str) -> Game {
        Game::new(Level::parse(map).unwrap())
    }

    fn play(game: &mut Game, moves: &str) -> Vec<StepOutcome> {
        moves
            .chars()
            .map(|ch| {
                game.step(match ch {
                    'w' => North,
                    's' => South,
                    'd' => East,
                    'a' => West,
                    _ => unreachable!(),
                })
            })
            .collect()
    }

    #[test]
    fn starts_on_the_start_at_time_zero() {
        let g = game(" WWW\nSE.XF\n WWW");
        assert_eq!(g.player(), Pos::new(0, 1));
        assert_eq!((g.now(), g.moves(), g.arrow(), g.status()), (0, 0, Arrow::Forward, Status::Playing));
    }

    #[test]
    fn walls_glass_and_turnstile_frame_block_without_spending_time() {
        let mut g = game(" WWWWW\nSE.g.XF\n WTWTW\n WWWWW");
        assert_eq!(g.step(West), StepOutcome::Blocked); // off the map
        assert_eq!(g.step(North), StepOutcome::Blocked); // nothing there
        assert_eq!(play(&mut g, "dd"), [StepOutcome::Moved, StepOutcome::Moved]);
        assert_eq!(g.step(East), StepOutcome::Blocked); // glass
        assert_eq!(g.step(South), StepOutcome::Blocked); // turnstile frame
        assert_eq!((g.moves(), g.now()), (2, 2));
    }

    #[test]
    fn crossing_the_mirror_reverses_the_arrow_without_spending_global_time() {
        let mut g = game("SEr__b.XF");
        let outcomes = play(&mut g, "dddd");
        assert_eq!(outcomes, [StepOutcome::Moved, StepOutcome::Moved, StepOutcome::Moved, StepOutcome::Inverted]);
        assert_eq!((g.arrow(), g.now(), g.player(), g.segments().len()), (Arrow::Inverted, 3, Pos::new(4, 0), 2));
        play(&mut g, "dd");
        assert_eq!((g.now(), g.moves()), (1, 6)); // two inverted moves took global time back to 1
        assert_eq!(
            g.segments()[1],
            Segment { arrow: Arrow::Inverted, start: 3, path: vec![Pos::new(4, 0), Pos::new(5, 0), Pos::new(6, 0)] }
        );
        assert_eq!(g.segments()[1].span(), (1, 3));
    }

    #[test]
    fn at_the_inversion_both_selves_stand_on_either_side_of_the_mirror() {
        let mut g = game("SEr__b.XF");
        play(&mut g, "dddd");
        assert_eq!(
            g.selves_at(3),
            [
                SelfAt { pos: Pos::new(3, 0), arrow: Arrow::Forward, active: false },
                SelfAt { pos: Pos::new(4, 0), arrow: Arrow::Inverted, active: true },
            ]
        );
    }

    #[test]
    fn turnstile_doors_close_while_anyone_is_inside() {
        let mut g = game("SEr__b.XF");
        play(&mut g, "dddd"); // inside at t = 3, inverted
        assert!(g.turnstile_open_at(2) && !g.turnstile_open_at(3) && g.turnstile_open_at(4));
        assert_eq!(g.step(East), StepOutcome::Moved); // out of the blue door at t = 2: open again
        // A forward self can't get in while an earlier self is inside.
        let mut g = game("SEr__b.XF");
        play(&mut g, "ddddaa"); // inverted at t = 3, then back across: forward again, still at t = 3
        assert_eq!(g.arrow(), Arrow::Forward);
        assert!(!g.turnstile_open_at(3));
    }

    #[test]
    fn earlier_selves_replay_at_the_current_time() {
        let mut g = game("SEr__b.XF");
        play(&mut g, "dddddd"); // inverted at t = 3, now at t = 1
        assert_eq!(
            g.selves_at(1),
            [
                SelfAt { pos: Pos::new(1, 0), arrow: Arrow::Forward, active: false }, // forward self, entering
                SelfAt { pos: Pos::new(6, 0), arrow: Arrow::Inverted, active: true },
            ]
        );
        assert!(g.selves_at(-5).is_empty());
    }

    #[test]
    fn the_exit_door_only_lets_you_through_while_it_is_open_and_the_finish_ends_the_level() {
        let mut g = game("SE.XF\n\nexit-closes: 3");
        assert_eq!(play(&mut g, "ddd"), [StepOutcome::Moved, StepOutcome::Moved, StepOutcome::Blocked]);
        let mut g = game("SE.XF\n\nexit-closes: 4");
        assert_eq!(play(&mut g, "dddd"), [StepOutcome::Moved, StepOutcome::Moved, StepOutcome::Moved, StepOutcome::Exited]);
        assert_eq!(g.status(), Status::Exited);
        assert_eq!(g.step(West), StepOutcome::Blocked);
    }

    #[test]
    fn solution_inverts_and_leaves_through_the_door_before_it_closed() {
        let mut g = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        // In through the entry door, up to the glass, and watch the exit door close at t = 9.
        assert!(play(&mut g, "dddddwwwwdd").iter().all(|&o| o == StepOutcome::Moved));
        assert_eq!((g.player(), g.now()), (Pos::new(7, 2), 11));
        assert!(!g.level().exit_open_at(g.now()));
        // Back to the turnstile, in, and across the mirror.
        play(&mut g, "aass");
        assert_eq!(play(&mut g, "ddd"), [StepOutcome::Moved, StepOutcome::Moved, StepOutcome::Inverted]);
        assert_eq!((g.player(), g.now(), g.arrow()), (Pos::new(8, 4), 17, Arrow::Inverted));
        // Inverted: out of the blue door and up to the exit, global time running backward.
        let outcomes = play(&mut g, "ddddddwwdd");
        assert_eq!(outcomes.last(), Some(&StepOutcome::Exited));
        assert_eq!((g.now(), g.moves()), (7, 28));
    }
}
