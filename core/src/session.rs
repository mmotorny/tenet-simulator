// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! A play session: the game plus everything around it that every front end shares — undo, the
//! level's par, the predicted rest of history, paradoxes, animations (door slides, the winning
//! logo flip, the universe collapsing) and the tape of consciousness that can be scrubbed after
//! winning.
//!
//! The universe is strongly deterministic: the screen always shows one consistent history, your
//! lived worldline plus the solver's prediction of the rest of it (so your future selves are
//! visible before you become them). Deviate from the prediction and the solver finds a new
//! consistent history; if none exists, you've caused a paradox.
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
/// The longest solution the par search and the prediction consider (in practice, the time
/// window keeps the search small).
const SEARCH_MOVES: usize = 10_000;
/// Collapse frames after a paradox (at 8 every cell has turned to static).
const COLLAPSE_STAGES: u32 = 8;
/// How collapsed the board stays afterwards: enough static to read as broken, little enough to
/// still see what happened.
const COLLAPSED: u32 = 3;

/// The bonus for replaying the solver's original solution exactly.
const BONUS: &str = "strong determinism";

/// Why the universe collapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paradox {
    /// A later self closed a door an earlier self had already walked through.
    BrokePast,
    /// No consistent history leads to the finish any more.
    NoWayOut,
}

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
    /// Show or hide the legend.
    ToggleLegend,
}

/// An animation frame waiting to be shown: what it changes on screen, and for how long.
#[derive(Clone, Debug, Default)]
struct Frame {
    door_overrides: Vec<(Pos, char)>,
    title: Option<Vec<Vec<Glyph>>>,
    collapse: Option<u32>,
    /// The strong determinism bonus being revealed: how many letters from each end are showing.
    reveal: Option<usize>,
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
    /// The moves of this attempt, matching `history`.
    path: Vec<Dir>,
    /// The history shown before the first move. Living your future selves exactly as you saw them
    /// there earns the "strong determinism" bonus.
    foreseen: Option<Game>,
    /// The whole history the universe would have: the lived game plus the predicted rest.
    prediction: Option<Game>,
    paradox: Option<Paradox>,
    tape: Option<Tape>,
    /// Animation frames still to show, front first.
    animation: VecDeque<Frame>,
    /// Whether the tape starts rewinding once the current animation is over (after winning).
    tape_pending: bool,
    legend: bool,
}

impl Session {
    pub fn new(level: Level) -> Self {
        let game = Game::new(level.clone());
        let solution = crate::solver::shortest_solution(&level, SEARCH_MOVES);
        let mut session = Self {
            par: solution.as_ref().map(|s| s.len() as u32),
            level,
            game,
            history: Vec::new(),
            path: Vec::new(),
            foreseen: None,
            prediction: None,
            paradox: None,
            tape: None,
            animation: VecDeque::new(),
            tape_pending: false,
            legend: false,
        };
        session.predict();
        session.foreseen = session.prediction.clone();
        session
    }

    /// Why the universe collapsed, if it did.
    pub fn paradox(&self) -> Option<Paradox> {
        self.paradox
    }

    /// Re-solves the rest of history after the lived game changed.
    fn predict(&mut self) {
        self.prediction = None;
        self.paradox = None;
        if self.game.status() == Status::Exited {
            return;
        }
        let found = if self.game.is_consistent() {
            crate::solver::completion(&self.game, SEARCH_MOVES).ok_or(Paradox::NoWayOut)
        } else {
            Err(Paradox::BrokePast)
        };
        match found {
            Ok(moves) => {
                let mut history = self.game.clone();
                for dir in moves {
                    history.step(dir);
                }
                self.prediction = Some(history);
            }
            Err(paradox) => {
                self.paradox = Some(paradox);
                for stage in 1..=COLLAPSE_STAGES {
                    self.animation.push_back(Frame { collapse: Some(stage), ms: ANIMATION_FRAME_MS, ..Frame::default() });
                }
            }
        }
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
                    self.path.pop();
                    let before = self.game.doors_at(self.game.now());
                    self.game = previous;
                    self.tape = None;
                    self.animate_doors(&before);
                    self.predict();
                }
            }
            Input::ToggleLegend => self.legend = !self.legend,
            Input::Restart => {
                self.game = Game::new(self.level.clone());
                self.history.clear();
                self.path.clear();
                self.tape = None;
                self.predict();
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
        if self.game.status() == Status::Exited || self.paradox.is_some() {
            return; // nothing to move once the level is over or the universe has collapsed
        }
        let before = self.game.doors_at(self.game.now());
        let snapshot = self.game.clone();
        if self.game.step(dir) == StepOutcome::Blocked {
            return;
        }
        self.history.push(snapshot);
        self.path.push(dir);
        self.animate_doors(&before);
        self.predict();
        if self.game.status() == Status::Exited {
            self.celebrate();
        }
    }

    /// Whether this attempt finished with every self after the first doing exactly what the
    /// universe showed it doing before the first move: the future selves you saw at the start,
    /// lived by memory, mirrored in time. The first stretch (the route to the mirror) is yours.
    pub fn strongly_determined(&self) -> bool {
        let Some(foreseen) = self.foreseen.as_ref() else { return false };
        self.game.status() == Status::Exited && self.game.segments().get(1..) == foreseen.segments().get(1..)
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
        let bonus = self.strongly_determined();
        let hidden = bonus.then_some(0); // the bonus stays hidden until its reveal
        self.animation.push_back(Frame { reveal: hidden, ms: 250, ..Frame::default() });
        // A half turn (the logo reads the same rotated).
        for height in (1..full).rev().chain(1..=full) {
            let title = Some(squashed(&logo, height));
            self.animation.push_back(Frame { title, reveal: hidden, ms: ANIMATION_FRAME_MS, ..Frame::default() });
        }
        if bonus {
            // "strong determinism" appears from both ends toward the middle, like the run itself.
            for k in 1..=BONUS.chars().count().div_ceil(2) {
                self.animation.push_back(Frame { reveal: Some(k), ms: ANIMATION_FRAME_MS, ..Frame::default() });
            }
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

    /// The screen as it should look right now. `over` are the hints after a paradox.
    pub fn screen(&self, playing: &[Hint], exited: &[Hint], over: &[Hint]) -> view::Screen {
        let frame = self.animation.front();
        let mut extras = view::Extras {
            par: self.par,
            door_overrides: frame.map_or(&[], |f| f.door_overrides.as_slice()),
            replay: self.tape.as_ref().map(Tape::replay),
            title: frame.and_then(|f| f.title.as_deref()),
            legend: self.legend,
            lived: None,
            collapse: frame.and_then(|f| f.collapse),
        };
        let separator = || view::colored(" · ", view::Color::Hint);

        if let Some(paradox) = self.paradox {
            extras.collapse = extras.collapse.or(Some(COLLAPSED));
            let mut screen = view::screen_with(&self.game, playing, exited, &extras);
            let reason = match paradox {
                Paradox::BrokePast => "✗ paradox: you closed a door your past self walked through",
                Paradox::NoWayOut => "✗ paradox: no consistent way out remains",
            };
            screen.status = view::colored(reason, view::Color::Key);
            if !over.is_empty() {
                screen.status.extend(separator());
                screen.status.extend(view::hint_glyphs(over));
            }
            return screen;
        }

        let Some(prediction) = self.prediction.as_ref().filter(|_| self.tape.is_none()) else {
            // The level is over: the lived history is the whole history.
            let mut screen = view::screen_with(&self.game, playing, exited, &extras);
            if self.game.status() == Status::Exited {
                screen.status = view::status_line(&self.game, playing, &[], self.par);
                if self.strongly_determined() {
                    screen.status.extend(separator());
                    // While it's revealed, the letters arriving from the start are red and those
                    // from the end blue: forward and inverted, meeting in the middle. Then plain.
                    let shown = frame.and_then(|f| f.reveal);
                    let n = BONUS.chars().count();
                    for (i, ch) in BONUS.chars().enumerate() {
                        let glyph = match shown {
                            None => Glyph::new(ch, view::Color::Key),
                            Some(k) if i < k => Glyph::new(ch, view::Color::Red),
                            Some(k) if i >= n - k.min(n) => Glyph::new(ch, view::Color::Blue),
                            Some(_) => Glyph::BLANK,
                        };
                        screen.status.push(glyph);
                    }
                }
                if !exited.is_empty() {
                    screen.status.extend(separator());
                    screen.status.extend(view::hint_glyphs(exited));
                }
            }
            return screen;
        };

        // Playing: show the whole predicted history at the lived moment, the lived self in control.
        let segments = self.game.segments();
        let controlled = segments.len() - 1;
        extras.replay = Some(Replay { t: self.game.now(), controlled });
        extras.lived = Some((segments.len(), segments[controlled].path.len()));
        let mut screen = view::screen_with(prediction, playing, exited, &extras);
        screen.status = view::status_line(&self.game, playing, exited, self.par);
        screen
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
        Session::new(Level::parse(crate::levels::FREEPORT).unwrap())
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
    fn the_legend_toggles() {
        let mut s = session();
        let rows = |s: &Session| s.screen(&[], &[], &[]).timeline.len();
        let timeline = rows(&s);
        s.input(Input::ToggleLegend);
        assert_eq!(s.screen(&[], &[], &[]).timeline[0][0].ch, '●');
        s.input(Input::ToggleLegend);
        assert_eq!(rows(&s), timeline);
    }

    fn plain_rows(rows: &[Vec<Glyph>]) -> Vec<String> {
        rows.iter().map(|r| r.iter().map(|g| view::monochrome(*g).ch).collect::<String>().trim_end().to_string()).collect()
    }

    #[test]
    fn the_future_self_is_visible_before_you_become_it() {
        let s = session();
        let screen = s.screen(&[], &[], &[]);
        let board = plain_rows(&screen.board);
        assert!(board[2].ends_with("⠀ ◇"), "{board:?}"); // you, later, on the finish at t = 0
        assert_eq!(plain_rows(&screen.timeline)[3], "  ◇╋╌╌╌╌╌╌╌┛"); // a predicted lane, dashed
    }

    #[test]
    fn wandering_before_the_turnstile_never_causes_a_paradox() {
        let mut s = session();
        play(&mut s, "dd");
        for _ in 0..40 {
            play(&mut s, "sw");
        }
        assert_eq!((s.paradox(), s.game().now()), (None, 82));
    }

    #[test]
    fn closing_a_door_your_past_self_walked_through_is_a_paradox() {
        let mut s = session();
        // Across the mirror, back onto the blue trapdoor at t = 7, and across again: you now stand
        // on the red trapdoor at the moment your past self walked through the red door.
        play(&mut s, "ddwwdddddddaa");
        assert_eq!(s.paradox(), Some(Paradox::BrokePast));
        play(&mut s, "d"); // nothing moves once the universe has collapsed
        assert_eq!(s.game().moves(), 13);
        s.input(Input::Undo);
        assert_eq!(s.paradox(), None);
    }

    #[test]
    fn strong_determinism_means_living_your_future_selves_as_foreseen() {
        let mut s = session();
        play(&mut s, PERFECT);
        assert!(s.strongly_determined());
        let mut other_route = session();
        play(&mut other_route, "ddwdwddddddddwwdddd"); // another way to the mirror: still yours to choose
        assert!(other_route.strongly_determined());
        let mut deviated = session();
        play(&mut deviated, "ddwwdddddddddwdwddd"); // the inverted half differs from what you saw
        assert_eq!(deviated.game().status(), Status::Exited);
        assert!(!deviated.strongly_determined());
    }

    #[test]
    fn the_bonus_is_revealed_from_both_ends() {
        let mut s = session();
        play(&mut s, PERFECT);
        let mut seen = Vec::new();
        while !s.on_tape() {
            let status: String = s.screen(&[], &[], &[]).status.iter().map(|g| g.ch).collect();
            seen.push(status.trim_end().to_string());
            s.advance();
        }
        assert!(seen.iter().any(|l| l.ends_with("· s                m")));
        assert!(seen.iter().any(|l| l.ends_with("· strong d  erminism")));
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
