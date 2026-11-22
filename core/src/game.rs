// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Game state and rules: worldlines, arrows of time, and the turnstile.
//!
//! Every move advances the protagonist's own (proper) time by one, and global time by one in the
//! direction of their arrow: +1 forward, −1 inverted. Crossing the turnstile's mirror (between the
//! two halves of its interior) moves the frontier of consciousness to the other side and reverses
//! the arrow without advancing global time; the next move runs in the new direction. The cells
//! on either side of the mirror are trapdoors: each turnstile door is closed while anyone stands
//! on the trapdoor on its side. A door can only be walked through while it is open, both now and
//! on arrival. The protagonist's history is a worldline made of segments, one per stretch between
//! inversions; earlier segments replay as other selves at whatever global time it is.
//!
//! Consistency is lenient for now: past selves are not obstacles, and what they saw isn't checked.

use crate::level::{Level, Pos, Side, Tile};

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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// The protagonist stepped onto the finish: the level is over.
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

    /// Every self present at global time `t`; the active one is the self being controlled now.
    pub fn selves_at(&self, t: i32) -> Vec<SelfAt> {
        self.selves_controlled(t, self.segments.len() - 1)
    }

    /// Every self present at global time `t`, with the self of segment `controlled` as the active
    /// one (for replays, which follow the frontier of consciousness through earlier segments).
    pub fn selves_controlled(&self, t: i32, controlled: usize) -> Vec<SelfAt> {
        self.segments
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.pos_at(t).map(|pos| SelfAt { pos, arrow: s.arrow, active: i == controlled }))
            .collect()
    }

    /// The frontier of consciousness through the whole worldline, in the protagonist's own order:
    /// the global time and segment of every position they occupied, from the start to now.
    pub fn consciousness(&self) -> Vec<(i32, usize)> {
        self.segments
            .iter()
            .enumerate()
            .flat_map(|(i, s)| (0..s.path.len()).map(move |k| (s.start + k as i32 * s.arrow.dt(), i)))
            .collect()
    }

    /// Whether the turnstile door on `side` is open at global time `t`: it is closed while anyone
    /// stands on the trapdoor on its side of the mirror.
    pub fn turnstile_door_open_at(&self, side: Side, t: i32) -> bool {
        let Some(turnstile) = self.level.turnstile() else { return true };
        let trapdoor = match side {
            Side::Red => turnstile.red_cell,
            Side::Blue => turnstile.blue_cell,
        };
        !self.segments.iter().any(|s| s.pos_at(t) == Some(trapdoor))
    }

    /// Every door and whether it is open at global time `t`: the entry and exit doors, then the
    /// turnstile's doors.
    pub fn doors_at(&self, t: i32) -> Vec<(Pos, bool)> {
        let mut doors = vec![(self.level.entry(), self.level.entry_open_at(t)), (self.level.exit(), self.level.exit_open_at(t))];
        if let Some(turnstile) = self.level.turnstile() {
            doors.push((turnstile.red_door, self.turnstile_door_open_at(Side::Red, t)));
            doors.push((turnstile.blue_door, self.turnstile_door_open_at(Side::Blue, t)));
        }
        doors
    }

    pub fn step(&mut self, dir: Dir) -> StepOutcome {
        if self.status == Status::Exited {
            return StepOutcome::Blocked;
        }
        let from = self.player();
        let target = dir.offset(from);
        let crossing = self.level.turnstile().is_some_and(|t| t.crosses_mirror(from, target));
        let now = self.now();
        // Crossing the mirror moves the frontier of consciousness without advancing global time.
        let arrow = if crossing { self.arrow().reversed() } else { self.arrow() };
        let arrival = if crossing { now } else { now + arrow.dt() };
        let walkable = match self.level.tile(target) {
            // A door must be open now and when you'd arrive: walking into a closed door is not a
            // move, and doors never open because someone walks into them.
            Some(Tile::Entry) => [now, arrival].iter().all(|&t| self.level.entry_open_at(t)),
            Some(Tile::Exit) => [now, arrival].iter().all(|&t| self.level.exit_open_at(t)),
            Some(Tile::TurnstileDoor(side)) => [now, arrival].iter().all(|&t| self.turnstile_door_open_at(side, t)),
            Some(tile) => tile.is_walkable(),
            None => false,
        };
        if !walkable {
            return StepOutcome::Blocked;
        }

        self.moves += 1;
        if crossing {
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

    /// S E r _ _ | _ _ b . X F: the mirror lies between x = 4 and x = 5.
    const BOOTH: &str = "SEr____b.XF";

    #[test]
    fn crossing_the_mirror_reverses_the_arrow_without_spending_global_time() {
        let mut g = game(BOOTH);
        let outcomes = play(&mut g, "ddddd");
        assert_eq!(outcomes[4], StepOutcome::Inverted);
        assert_eq!((g.arrow(), g.now(), g.player(), g.segments().len()), (Arrow::Inverted, 4, Pos::new(5, 0), 2));
        play(&mut g, "dd");
        assert_eq!((g.now(), g.moves()), (2, 7)); // two inverted moves took global time back to 2
        assert_eq!(
            g.segments()[1],
            Segment { arrow: Arrow::Inverted, start: 4, path: vec![Pos::new(5, 0), Pos::new(6, 0), Pos::new(7, 0)] }
        );
        assert_eq!(g.segments()[1].span(), (2, 4));
    }

    #[test]
    fn each_turnstile_door_is_closed_while_its_trapdoor_is_occupied() {
        let mut g = game(BOOTH);
        play(&mut g, "dddd"); // on the red trapdoor at t = 4
        assert!(!g.turnstile_door_open_at(Side::Red, 4) && g.turnstile_door_open_at(Side::Blue, 4));
        play(&mut g, "d"); // across, onto the blue trapdoor at the same t = 4
        assert!(!g.turnstile_door_open_at(Side::Red, 4) && !g.turnstile_door_open_at(Side::Blue, 4));
        assert!(g.turnstile_door_open_at(Side::Red, 3) && g.turnstile_door_open_at(Side::Blue, 3));
        assert_eq!(play(&mut g, "dd"), [StepOutcome::Moved, StepOutcome::Moved]); // off it and out
    }

    #[test]
    fn doors_must_be_open_now_and_on_arrival() {
        // Arriving at t = 3 when the exit closes at 3.
        let mut g = game("SE.XF\n\nexit-closes: 3");
        assert_eq!(play(&mut g, "ddd"), [StepOutcome::Moved, StepOutcome::Moved, StepOutcome::Blocked]);
        // Inverted next to the exit at t = 4: open on arrival (t = 3) but not now, so blocked.
        let mut g = game("SE...r____b.XF\n\nexit-closes: 4");
        play(&mut g, "dddddddd"); // across the mirror at t = 7
        play(&mut g, "ddd"); // out of the blue door, next to the exit at t = 4
        assert_eq!((g.player(), g.now()), (Pos::new(11, 0), 4));
        assert_eq!(g.step(East), StepOutcome::Blocked);
        play(&mut g, "ad"); // wait two moves: now t = 2
        assert_eq!(play(&mut g, "dd"), [StepOutcome::Moved, StepOutcome::Exited]);
    }

    #[test]
    fn earlier_selves_replay_at_the_current_time() {
        let mut g = game(BOOTH);
        play(&mut g, "ddddddd"); // inverted, on the blue door at t = 2
        assert_eq!(
            g.selves_at(2),
            [
                SelfAt { pos: Pos::new(2, 0), arrow: Arrow::Forward, active: false }, // the forward self, on the red door
                SelfAt { pos: Pos::new(7, 0), arrow: Arrow::Inverted, active: true },
            ]
        );
        assert!(g.selves_at(-5).is_empty());
    }

    #[test]
    fn consciousness_follows_the_protagonist_through_the_inversion() {
        let mut g = game(BOOTH);
        play(&mut g, "ddddddd");
        let c = g.consciousness();
        assert_eq!(c.len(), 8); // every position held, in the protagonist's own order
        assert_eq!((c[0], c[4], c[5], c[7]), ((0, 0), (4, 0), (4, 1), (2, 1)));
    }

    #[test]
    fn the_finish_ends_the_level() {
        let mut g = game("SE.XF\n\nexit-closes: 4");
        assert_eq!(play(&mut g, "dddd").last(), Some(&StepOutcome::Exited));
        assert_eq!(g.status(), Status::Exited);
        assert_eq!(g.step(West), StepOutcome::Blocked);
    }

    #[test]
    fn perfect_play_is_a_palindrome() {
        let mut g = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        let outcomes = play(&mut g, "ddwwdddddddddwwdddd");
        assert_eq!(outcomes.last(), Some(&StepOutcome::Exited));
        // 9 moves to the mirror, the crossing, 9 inverted moves out: back at t = 0 on the finish,
        // while the forward self stands on the start.
        assert_eq!((g.now(), g.moves(), g.segments().len()), (0, 19, 2));
        assert_eq!(g.segments()[0].path[0], g.level().start());
        assert_eq!(g.player(), g.level().finish());
    }
}
