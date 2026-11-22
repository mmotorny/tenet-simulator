// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! A play session: the game plus everything around it that every front end shares — undo, the
//! level's par, animations (door slides, the winning logo flip) and the tape of consciousness
//! that can be scrubbed after winning.
//!
//! Front ends feed [`Input`]s, draw [`Session::screen`], and call [`Session::advance`] whenever
//! [`Session::next_deadline`] passes. Animations are timed in milliseconds and skipped by any
//! input, so the session never blocks.

use std::collections::VecDeque;

use crate::game::{Dir, Game, Status, StepOutcome};
use crate::level::{Level, Pos};
use crate::view::{self, Glyph, Hint, Replay};

/// One frame "on twos" (12 drawings per second, the classical animation standard): door slides
/// and the logo flip.
pub const ANIMATION_FRAME_MS: u32 = 83;
/// One tape frame at 1×, "on threes" (8 frames per second, as in limited animation).
pub const TAPE_FRAME_MS: u32 = 125;
/// The longest solution the par search considers.
const PAR_SEARCH_MOVES: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Move(Dir),
    Undo,
    Restart,
    /// Tape: play toward the start; again for 2× and 4× (J in video editors).
    Rewind,
    /// Tape: pause (K).
    Pause,
    /// Tape: play toward the end; again for 2× and 4× (L).
    Play,
    /// Tape: play or pause, resuming the last direction.
    TogglePlay,
    /// Tape: one frame toward the start.
    StepBack,
    /// Tape: one frame toward the end.
    StepForward,
}

/// An animation frame waiting to be shown: what it changes on screen, and for how long.
#[derive(Clone, Debug, Default)]
struct Frame {
    door_overrides: Vec<(Pos, char)>,
    title: Option<Vec<Vec<Glyph>>>,
    ms: u32,
}

/// The replay after winning: the worldline as one tape of consciousness (the swimlanes joined at
/// their inversions), scrubbed or played in either direction.
#[derive(Clone, Debug)]
struct Tape {
    /// The frontier of consciousness in the protagonist's own order: (global time, segment).
    frames: Vec<(i32, usize)>,
    pos: usize,
    /// Playing toward the start (-1) or the end (+1), or paused.
    playing: Option<i8>,
    /// 1×, 2× or 4×.
    speed: u32,
    /// The direction of the last step or play, which [`Input::TogglePlay`] resumes.
    direction: i8,
}

impl Tape {
    fn new(game: &Game) -> Self {
        let frames = game.consciousness();
        let pos = frames.len() - 1;
        Self { frames, pos, playing: None, speed: 1, direction: -1 }
    }

    fn replay(&self) -> Replay {
        let (t, controlled) = self.frames[self.pos];
        Replay { t, controlled }
    }

    /// Plays in `direction` like a video editor's J and L: the same direction again speeds up,
    /// the other one starts over at 1×.
    fn shuttle(&mut self, direction: i8) {
        self.speed = if self.playing == Some(direction) { (self.speed * 2).min(4) } else { 1 };
        self.playing = Some(direction);
        self.direction = direction;
    }

    fn pause(&mut self) {
        self.playing = None;
        self.speed = 1;
    }

    /// Moves one frame in `direction`; false at either end of the tape.
    fn step(&mut self, direction: i8) -> bool {
        match self.pos.checked_add_signed(direction as isize).filter(|&p| p < self.frames.len()) {
            Some(pos) => {
                self.pos = pos;
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Session {
    level: Level,
    game: Game,
    par: Option<u32>,
    /// Every earlier state of this attempt, for undo.
    history: Vec<Game>,
    tape: Option<Tape>,
    /// Animation frames still to show, front first.
    animation: VecDeque<Frame>,
    /// Whether the tape starts rewinding once the current animation is over (after winning).
    tape_pending: bool,
}

impl Session {
    pub fn new(level: Level) -> Self {
        let par = crate::solver::par(&level, PAR_SEARCH_MOVES);
        Self::with_par(level, par)
    }

    /// A session with a known par, skipping the search.
    pub fn with_par(level: Level, par: Option<u32>) -> Self {
        let game = Game::new(level.clone());
        Self { level, game, par, history: Vec::new(), tape: None, animation: VecDeque::new(), tape_pending: false }
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    pub fn par(&self) -> Option<u32> {
        self.par
    }

    /// Whether the tape is showing (after winning).
    pub fn on_tape(&self) -> bool {
        self.tape.is_some()
    }

    pub fn input(&mut self, input: Input) {
        self.skip_animation();
        match input {
            Input::Move(dir) => self.step(dir),
            Input::Undo => {
                if let Some(previous) = self.history.pop() {
                    let before = self.game.doors_at(self.game.now());
                    self.game = previous;
                    self.tape = None;
                    self.animate_doors(&before);
                }
            }
            Input::Restart => {
                self.game = Game::new(self.level.clone());
                self.history.clear();
                self.tape = None;
            }
            tape_input => {
                let Some(tape) = self.tape.as_mut() else { return };
                match tape_input {
                    Input::Rewind => tape.shuttle(-1),
                    Input::Play => tape.shuttle(1),
                    Input::Pause => tape.pause(),
                    Input::TogglePlay if tape.playing.is_some() => tape.pause(),
                    Input::TogglePlay => tape.shuttle(tape.direction),
                    Input::StepBack | Input::StepForward => {
                        let direction = if tape_input == Input::StepBack { -1 } else { 1 };
                        tape.pause();
                        tape.direction = direction;
                        tape.step(direction);
                    }
                    _ => unreachable!("handled above"),
                }
            }
        }
    }

    fn step(&mut self, dir: Dir) {
        if self.game.status() == Status::Exited {
            return; // nothing to move once the level is over
        }
        let before = self.game.doors_at(self.game.now());
        let snapshot = self.game.clone();
        if self.game.step(dir) == StepOutcome::Blocked {
            return;
        }
        self.history.push(snapshot);
        self.animate_doors(&before);
        if self.game.status() == Status::Exited {
            self.celebrate();
        }
    }

    /// Queues the slide of every door that opened or closed since `before`.
    fn animate_doors(&mut self, before: &[(Pos, bool)]) {
        let changed: Vec<(Pos, bool)> = self
            .game
            .doors_at(self.game.now())
            .into_iter()
            .zip(before)
            .filter(|((_, open), (_, was_open))| open != was_open)
            .map(|((pos, open), _)| (pos, open))
            .collect();
        if changed.is_empty() {
            return;
        }
        // DOOR_FRAMES runs open → closed: forward to close, backward to open.
        let last = view::DOOR_FRAMES.len() - 1;
        for k in 1..last {
            let door_overrides =
                changed.iter().map(|&(pos, open)| (pos, view::DOOR_FRAMES[if open { last - k } else { k }])).collect();
            self.animation.push_back(Frame { door_overrides, ms: ANIMATION_FRAME_MS, ..Frame::default() });
        }
    }

    /// The winning celebration: the title logo flips a half turn (it reads the same rotated),
    /// then the tape starts rewinding along the frontier of consciousness.
    fn celebrate(&mut self) {
        let logo = crate::title::logo();
        let full = logo.len();
        self.animation.push_back(Frame { ms: 250, ..Frame::default() });
        for height in (1..full).rev().chain(1..=full) {
            self.animation.push_back(Frame { title: Some(squashed(&logo, height)), ms: ANIMATION_FRAME_MS, ..Frame::default() });
        }
        self.animation.push_back(Frame { ms: 400, ..Frame::default() });
        self.tape_pending = true;
    }

    /// Drops any animation still to show, leaving the screen in its settled state (for output
    /// that isn't animated, such as scripted runs).
    pub fn settle(&mut self) {
        self.animation.clear();
    }

    fn skip_animation(&mut self) {
        while !self.animation.is_empty() {
            self.advance();
        }
    }

    /// Milliseconds until the screen changes by itself (an animation frame ends or the tape plays
    /// on), or `None` if it's waiting for input.
    pub fn next_deadline(&self) -> Option<u32> {
        if let Some(frame) = self.animation.front() {
            return Some(frame.ms);
        }
        self.tape.as_ref().filter(|t| t.playing.is_some()).map(|t| TAPE_FRAME_MS / t.speed)
    }

    /// Moves time on to the next deadline: the next animation frame, or the next tape frame.
    pub fn advance(&mut self) {
        if self.animation.pop_front().is_some() {
            if self.animation.is_empty() && self.tape_pending {
                self.tape_pending = false;
                let mut tape = Tape::new(&self.game);
                tape.playing = Some(-1); // rewind by itself; the player can take over
                self.tape = Some(tape);
            }
            return;
        }
        if let Some(tape) = self.tape.as_mut() {
            if let Some(direction) = tape.playing {
                if !tape.step(direction) {
                    tape.pause();
                }
            }
        }
    }

    /// The screen as it should look right now.
    pub fn screen(&self, playing: &[Hint], exited: &[Hint]) -> view::Screen {
        let frame = self.animation.front();
        let extras = view::Extras {
            par: self.par,
            door_overrides: frame.map_or(&[], |f| f.door_overrides.as_slice()),
            replay: self.tape.as_ref().map(Tape::replay),
            title: frame.and_then(|f| f.title.as_deref()),
        };
        view::screen_with(&self.game, playing, exited, &extras)
    }
}

/// The logo squashed to `height` rows as the slab turns edge-on: its rows sampled evenly and
/// centred in the logo's own height (one row is the slab seen edge-on).
fn squashed(logo: &[Vec<Glyph>], height: usize) -> Vec<Vec<Glyph>> {
    let full = logo.len();
    let width = logo[0].len();
    let rows: Vec<Vec<Glyph>> = match height {
        0 | 1 => vec![vec![Glyph::new('━', view::Color::Slab); width]],
        h => (0..h).map(|i| logo[(i * (full - 1) + (h - 1) / 2) / (h - 1)].clone()).collect(),
    };
    let above = (full - rows.len()) / 2;
    let mut frame = vec![vec![Glyph::BLANK; width]; above];
    frame.extend(rows);
    frame.resize(full, vec![Glyph::BLANK; width]);
    frame
}

#[cfg(test)]
mod tests {
    use super::*;
    use Dir::*;

    fn session() -> Session {
        Session::with_par(Level::parse(crate::levels::FREEPORT).unwrap(), Some(19))
    }

    fn play(s: &mut Session, moves: &str) {
        for ch in moves.chars() {
            let dir = match ch {
                'w' => North,
                's' => South,
                'd' => East,
                _ => West,
            };
            s.input(Input::Move(dir));
        }
    }

    const PERFECT: &str = "ddwwdddddddddwwdddd";

    #[test]
    fn undo_restores_the_previous_state_and_move_count() {
        let mut s = session();
        play(&mut s, "ddwwddddd");
        assert_eq!(s.game().moves(), 9);
        s.input(Input::Undo);
        s.input(Input::Undo);
        assert_eq!((s.game().moves(), s.game().now()), (7, 7));
        s.input(Input::Restart);
        s.input(Input::Undo); // nothing to undo after a restart
        assert_eq!(s.game().moves(), 0);
    }

    #[test]
    fn doors_animate_when_they_change() {
        let mut s = session();
        play(&mut s, "dd"); // t = 2
        assert_eq!(s.next_deadline(), None);
        play(&mut s, "w"); // t = 3: both room doors close
        assert_eq!(s.next_deadline(), Some(ANIMATION_FRAME_MS));
        let mut frames = 0;
        while s.next_deadline().is_some() {
            s.advance();
            frames += 1;
        }
        assert_eq!(frames, view::DOOR_FRAMES.len() - 2); // the in-between frames
    }

    #[test]
    fn winning_celebrates_then_rewinds_the_tape() {
        let mut s = session();
        play(&mut s, PERFECT);
        assert_eq!(s.game().status(), Status::Exited);
        assert!(!s.on_tape());
        while !s.on_tape() {
            s.advance(); // the logo flip
        }
        assert_eq!(s.next_deadline(), Some(TAPE_FRAME_MS)); // rewinding by itself
        s.input(Input::Pause);
        assert_eq!(s.next_deadline(), None);
        s.input(Input::StepBack);
        s.input(Input::Play);
        s.input(Input::Play);
        assert_eq!(s.next_deadline(), Some(TAPE_FRAME_MS / 2)); // 2×
    }

    #[test]
    fn any_input_skips_an_animation() {
        let mut s = session();
        play(&mut s, PERFECT);
        s.input(Input::Pause); // skips the flip; the tape starts and is paused at once
        assert!(s.on_tape());
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn undo_after_winning_leaves_the_tape() {
        let mut s = session();
        play(&mut s, PERFECT);
        s.input(Input::Undo);
        assert!(!s.on_tape());
        assert_eq!((s.game().status(), s.game().moves()), (Status::Playing, 18));
    }

    #[test]
    fn squashed_logo_keeps_its_height() {
        let logo = crate::title::logo();
        for h in 1..=logo.len() {
            let frame = squashed(&logo, h);
            assert_eq!(frame.len(), logo.len());
            assert_eq!(frame.iter().filter(|r| r.iter().any(|g| g.ch != ' ')).count(), h);
        }
        assert_eq!(squashed(&logo, logo.len()), logo);
    }
}
