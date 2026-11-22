// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! A play session: the game plus everything around it that every front end shares — the tape of
//! consciousness, the level's par, the predicted rest of history, paradoxes, animations (door
//! slides, the winning logo flip, the universe collapsing) and pointer gestures.
//!
//! The universe is strongly deterministic: the screen always shows one consistent history, your
//! lived worldline plus the solver's prediction of the rest of it (so your future selves are
//! visible before you become them). Deviate from the prediction and the solver finds a new
//! consistent history; if none exists, you've caused a paradox.
//!
//! Every move is a moment on the tape of consciousness, in your own order. The tape can be
//! rewound and played at any time: the history stays what it is (everything on the tape is
//! committed, the rest predicted from its last moment), and only your consciousness moves along
//! it. Moving from a rewound moment cuts the tape there. Winning plays it back by itself.
//!
//! Front ends feed [`Input`]s (or [`Pointer`] events), draw [`Session::screen`], and call
//! [`Session::advance`] whenever [`Session::next_deadline`] passes. Animations are timed in
//! milliseconds and skipped by any input, so the session never blocks.

use std::collections::VecDeque;

use crate::game::{Dir, Game, Status, StepOutcome};
use crate::level::{Level, Pos};
use crate::view::{self, Glyph, Hint, Region, Replay};

/// One frame "on twos" (12 drawings per second, the classical animation standard): door slides
/// and the logo flip.
pub const ANIMATION_FRAME_MS: u32 = 83;
/// One tape frame at 1×, "on threes" (8 frames per second, as in limited animation).
pub const TAPE_FRAME_MS: u32 = 125;
/// The longest solution the par search and the prediction consider. A fixed budget for now: past
/// it, "no way out" only means none within this many moves.
const SEARCH_MOVES: usize = 500;
/// Collapse frames after a paradox (at 8 every cell has turned to static).
const COLLAPSE_STAGES: u32 = 8;
/// How collapsed the board stays afterwards: enough static to read as broken, little enough to
/// still see what happened.
const COLLAPSED: u32 = 3;
/// How far (in cell widths) a pointer must travel before it's a swipe rather than a tap.
const SWIPE_CELLS: f32 = 3.0;
/// How tall a cell is, in cell widths (terminal cells are about twice as tall as wide).
const CELL_ASPECT: f32 = 2.0;
/// How far (in cells) a drag along the timeline goes per moment of the tape.
const SCRUB_CELLS: f32 = 1.0;
/// How much of a drag's end the fling speed is measured over.
const FLING_WINDOW_MS: f64 = 100.0;
/// A fling slows down like a scrolled list: its speed falls by e every this many milliseconds.
const FLING_DECAY_MS: f32 = 325.0;
/// The slowest fling, in moments per second: slower releases just stop, slower flings end.
const FLING_MIN: f32 = 4.0;

/// The bonus for replaying the solver's original solution exactly.
const BONUS: &str = "strong determinism";

/// Why the universe collapsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paradox {
    /// A later self closed a door an earlier self had already walked through.
    BrokePast,
    /// No consistent history leads to the finish within the search budget.
    NoWayOut,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Move(Dir),
    /// Start over: the tape is erased back to its first moment (rewinding keeps it).
    Restart,
    /// Tape: play toward the start; again for 2× and 4× (J in video editors).
    Rewind,
    /// Tape: pause (K).
    Pause,
    /// Tape: play toward the end; again for 2× and 4× (L).
    Play,
    /// Tape: play or pause, resuming the last direction.
    TogglePlay,
    /// Tape: one moment toward the start.
    StepBack,
    /// Tape: one moment toward the end.
    StepForward,
    /// Show or hide the legend.
    ToggleLegend,
    /// Leave the game (front ends that can, check [`Session::quitting`]).
    Quit,
}

/// The input a key stands for, as a hint names it: a character (`w`, `↑`, `,`) or a word (`esc`).
/// Front ends map their keys through this, so the keyboard and the clickable toolbar agree.
pub fn key_input(key: &str) -> Option<Input> {
    Some(match key {
        "↑" | "w" => Input::Move(Dir::North),
        "↓" | "s" => Input::Move(Dir::South),
        "→" | "d" => Input::Move(Dir::East),
        "←" | "a" => Input::Move(Dir::West),
        "r" => Input::Restart,
        "?" => Input::ToggleLegend,
        "j" => Input::Rewind,
        "k" => Input::Pause,
        "l" => Input::Play,
        " " => Input::TogglePlay,
        "," => Input::StepBack,
        "." => Input::StepForward,
        "q" | "esc" => Input::Quit,
        _ => return None,
    })
}

/// A pointer (mouse or touch) event, at a position in grid cells (fractions allowed) of the grid
/// the screen was laid out on, and a time in milliseconds (from any fixed origin). A gesture
/// starts on the board or the timeline: swipe the board to move; drag the timeline to scroll
/// through the tape, which flings on if let go while moving. The wheel scrolls the tape too.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pointer {
    /// Pressed, over `region` (see [`view::Screen::region_at`]).
    Down { x: f32, y: f32, ms: f64, region: Option<Region> },
    Move { x: f32, y: f32, ms: f64 },
    Up { x: f32, y: f32, ms: f64 },
    /// The wheel turned, in notches (fractions allowed): positive for later moments (down or
    /// right), negative for earlier ones.
    Wheel { notches: f32 },
    /// The mouse is over `region` (or nothing): toolbar targets light up.
    Hover { region: Option<Region> },
    /// A mouse click on `region`: it clicks toolbar targets, and nothing else.
    Click { region: Option<Region> },
}

/// A gesture in progress.
#[derive(Clone, Debug)]
struct Gesture {
    region: Region,
    x: f32,
    y: f32,
    /// Moments scrolled so far (timeline drags).
    scrolled: i32,
    /// Recent positions along the timeline and when, to measure a fling.
    trail: Vec<(f32, f64)>,
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

/// One moment of the tape: the lived game then, and the history the universe would have from
/// there (what's shown while it's the tape's last moment).
#[derive(Clone, Debug)]
struct Moment {
    game: Game,
    /// The whole history: the lived game plus the predicted rest (none once finished or broken).
    prediction: Option<Game>,
    paradox: Option<Paradox>,
}

#[derive(Clone, Debug)]
pub struct Session {
    par: Option<u32>,
    /// The tape of consciousness: every moment of this attempt, the start first.
    tape: Vec<Moment>,
    /// The moment showing.
    at: usize,
    /// Playing toward the start (-1) or the end (+1), or paused.
    playing: Option<i8>,
    /// 1×, 2× or 4×.
    speed: u32,
    /// The direction of the last step or play, which [`Input::TogglePlay`] resumes.
    direction: i8,
    /// The history shown before the first move. Living your future selves exactly as you saw them
    /// there earns the "strong determinism" bonus.
    foreseen: Option<Game>,
    /// Animation frames still to show, front first.
    animation: VecDeque<Frame>,
    /// Whether the tape starts rewinding once the current animation is over (after winning).
    rewind_pending: bool,
    legend: bool,
    /// The first global time the timeline shows, when it's too long to show whole.
    scroll: i32,
    gesture: Option<Gesture>,
    /// A fling running on after a drag: its speed in moments per second, signed like the tape.
    fling: Option<f32>,
    /// Wheel turns not yet worth a whole moment.
    wheel: f32,
    /// The toolbar target under the mouse.
    hover: Option<usize>,
    quitting: bool,
}

impl Session {
    pub fn new(level: Level) -> Self {
        let game = Game::new(level.clone());
        let solution = crate::solver::shortest_solution(&level, SEARCH_MOVES);
        let start = predicted(game, &[]);
        let mut session = Self {
            par: solution.as_ref().map(|s| s.len() as u32),
            foreseen: start.prediction.clone(),
            tape: vec![start],
            at: 0,
            playing: None,
            speed: 1,
            direction: -1,
            animation: VecDeque::new(),
            rewind_pending: false,
            legend: false,
            scroll: 0,
            gesture: None,
            fling: None,
            wheel: 0.0,
            hover: None,
            quitting: false,
        };
        session.follow();
        session
    }

    fn moment(&self) -> &Moment {
        &self.tape[self.at]
    }

    fn last(&self) -> &Moment {
        self.tape.last().expect("the tape always has its start")
    }

    /// The whole history, the same from every moment of the tape: everything on it, plus the rest
    /// as predicted from its last moment.
    fn history(&self) -> &Game {
        let last = self.last();
        last.prediction.as_ref().unwrap_or(&last.game)
    }

    /// The lived game at the moment showing.
    pub fn game(&self) -> &Game {
        &self.moment().game
    }

    /// Why the universe collapsed at the moment showing, if it did.
    pub fn paradox(&self) -> Option<Paradox> {
        self.moment().paradox
    }

    pub fn par(&self) -> Option<u32> {
        self.par
    }

    /// Whether the player asked to leave.
    pub fn quitting(&self) -> bool {
        self.quitting
    }

    pub fn input(&mut self, input: Input) {
        self.skip_animation();
        self.fling = None;
        match input {
            Input::Move(dir) => self.step(dir),
            Input::Quit => self.quitting = true,
            Input::ToggleLegend => self.legend = !self.legend,
            Input::Restart => {
                self.pause();
                self.tape.truncate(1);
                self.at = 0;
            }
            Input::Rewind => self.shuttle(-1),
            Input::Play => self.shuttle(1),
            Input::Pause => self.pause(),
            Input::TogglePlay if self.playing.is_some() => self.pause(),
            Input::TogglePlay => self.shuttle(self.direction),
            Input::StepBack | Input::StepForward => {
                let direction = if input == Input::StepBack { -1 } else { 1 };
                self.pause();
                self.direction = direction;
                self.seek(direction);
            }
        }
        self.follow();
    }

    /// Plays the tape in `direction` like a video editor's J and L: the same direction again
    /// speeds up, the other one starts over at 1×.
    fn shuttle(&mut self, direction: i8) {
        self.speed = if self.playing == Some(direction) { (self.speed * 2).min(4) } else { 1 };
        self.playing = Some(direction);
        self.direction = direction;
    }

    fn pause(&mut self) {
        self.playing = None;
        self.speed = 1;
    }

    /// Moves one moment along the tape in `direction`; false at either end.
    fn seek(&mut self, direction: i8) -> bool {
        match self.at.checked_add_signed(direction as isize).filter(|&at| at < self.tape.len()) {
            Some(at) => {
                self.at = at;
                true
            }
            None => false,
        }
    }

    fn step(&mut self, dir: Dir) {
        let now = self.moment();
        if now.game.status() == Status::Exited || now.paradox.is_some() {
            return; // nothing to move once the level is over or the universe has collapsed
        }
        let mut game = now.game.clone();
        if game.step(dir) == StepOutcome::Blocked {
            return;
        }
        let wanted = mirrored(&now.game, self.last().prediction.as_ref(), &game);
        // Doors as shown: in the whole history, which may already have someone on a trapdoor.
        let before = self.history().doors_at(now.game.now());
        let moment = predicted(game, &wanted);
        // Moving from a rewound moment cuts the tape there.
        self.pause();
        self.tape.truncate(self.at + 1);
        self.tape.push(moment);
        self.at += 1;
        self.animate_doors(&before);
        if self.paradox().is_some() {
            for stage in 1..=COLLAPSE_STAGES {
                self.animation.push_back(Frame { collapse: Some(stage), ms: ANIMATION_FRAME_MS, ..Frame::default() });
            }
        }
        if self.game().status() == Status::Exited {
            self.celebrate();
        }
    }

    /// Scrolls the timeline, when it's too long to show whole, to keep the moment showing in view.
    fn follow(&mut self) {
        let shown = self.history();
        let width = view::render(shown).first().map_or(0, Vec::len);
        self.scroll = view::scroll_timeline(shown, self.scroll, self.game().now(), width);
    }

    /// Whether this attempt finished with every self after the first doing exactly what the
    /// universe showed it doing before the first move: the future selves you saw at the start,
    /// lived by memory, mirrored in time. The first stretch (the route to the mirror) is yours.
    pub fn strongly_determined(&self) -> bool {
        let Some(foreseen) = self.foreseen.as_ref() else { return false };
        self.game().status() == Status::Exited && self.game().segments().get(1..) == foreseen.segments().get(1..)
    }

    /// Queues the slide of every door shown opening or closing since `before`.
    fn animate_doors(&mut self, before: &[(Pos, bool)]) {
        let changed: Vec<(Pos, bool)> = self
            .history()
            .doors_at(self.game().now())
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
    /// then the tape starts rewinding.
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
        self.rewind_pending = true;
    }

    /// Drops any animation still to show, leaving the screen in its settled state (for output
    /// that isn't animated, such as scripted runs).
    pub fn settle(&mut self) {
        self.animation.clear();
        self.rewind_pending = false;
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
        if let Some(speed) = self.fling {
            return Some(((1000.0 / speed.abs()) as u32).max(1));
        }
        self.playing.map(|_| TAPE_FRAME_MS / self.speed)
    }

    /// Moves time on to the next deadline: the next animation frame, or the next tape frame.
    pub fn advance(&mut self) {
        if self.animation.pop_front().is_some() {
            if self.animation.is_empty() && self.rewind_pending {
                self.rewind_pending = false;
                self.shuttle(-1); // rewind by itself; the player can take over
            }
            return;
        }
        if let Some(speed) = self.fling {
            let moved = self.seek(if speed > 0.0 { 1 } else { -1 });
            let slower = speed * (-1000.0 / speed.abs() / FLING_DECAY_MS).exp();
            self.fling = (moved && slower.abs() >= FLING_MIN).then_some(slower);
            self.follow();
            return;
        }
        if let Some(direction) = self.playing {
            if !self.seek(direction) {
                self.pause();
            }
            self.follow();
        }
    }

    /// Feeds a pointer event; returns whether it belongs to a gesture (so a front end can keep
    /// the platform from also scrolling or refreshing the page).
    pub fn pointer(&mut self, event: Pointer) -> bool {
        match event {
            Pointer::Wheel { notches } => {
                self.wheel += notches;
                while self.wheel.abs() >= 1.0 {
                    let later = self.wheel > 0.0;
                    self.input(if later { Input::StepForward } else { Input::StepBack });
                    self.wheel -= if later { 1.0 } else { -1.0 };
                }
                true
            }
            Pointer::Hover { region } => {
                self.hover = match region {
                    Some(Region::Toolbar { target, .. }) => Some(target),
                    _ => None,
                };
                self.hover.is_some()
            }
            Pointer::Click { region } => match region {
                Some(Region::Toolbar { input, .. }) => {
                    self.input(input);
                    true
                }
                _ => false,
            },
            Pointer::Down { x, y, ms, region } => {
                self.fling = None; // touching stops a fling, like a scrolled list
                self.gesture = region.map(|region| Gesture { region, x, y, scrolled: 0, trail: vec![(x, ms)] });
                self.gesture.is_some()
            }
            Pointer::Move { x, y, ms } | Pointer::Up { x, y, ms } => {
                let Some(mut gesture) = self.gesture.take() else { return false };
                let up = matches!(event, Pointer::Up { .. });
                match gesture.region {
                    Region::Timeline => {
                        // The tape follows the finger: drag left for later moments.
                        let target = ((gesture.x - x) / SCRUB_CELLS) as i32;
                        while gesture.scrolled != target {
                            let later = target > gesture.scrolled;
                            self.input(if later { Input::StepForward } else { Input::StepBack });
                            gesture.scrolled += if later { 1 } else { -1 };
                        }
                        gesture.trail.retain(|&(_, t)| ms - t <= FLING_WINDOW_MS);
                        gesture.trail.push((x, ms));
                        if up {
                            let (x0, t0) = gesture.trail[0];
                            let elapsed = (ms - t0) as f32;
                            let speed = if elapsed > 0.0 { (x0 - x) / SCRUB_CELLS / elapsed * 1000.0 } else { 0.0 };
                            self.fling = (speed.abs() >= FLING_MIN).then_some(speed);
                        }
                    }
                    Region::Board if up => {
                        let (dx, dy) = (x - gesture.x, (y - gesture.y) * CELL_ASPECT);
                        if dx.abs().max(dy.abs()) >= SWIPE_CELLS {
                            let dir = if dx.abs() > dy.abs() {
                                if dx > 0.0 { Dir::East } else { Dir::West }
                            } else if dy > 0.0 {
                                Dir::South
                            } else {
                                Dir::North
                            };
                            self.input(Input::Move(dir));
                        }
                    }
                    Region::Board => {}
                    Region::Toolbar { input, .. } if up => {
                        let (dx, dy) = (x - gesture.x, (y - gesture.y) * CELL_ASPECT);
                        if dx.abs().max(dy.abs()) < SWIPE_CELLS {
                            self.input(input); // a tap
                        }
                    }
                    Region::Toolbar { .. } => {}
                }
                if !up {
                    self.gesture = Some(gesture);
                }
                true
            }
        }
    }

    /// The screen as it should look right now. `over` are the hints after a paradox. Their keys
    /// on the status line are clickable (see [`view::toolbar_targets`]).
    pub fn screen(&self, playing: &[Hint], exited: &[Hint], over: &[Hint]) -> view::Screen {
        let mut screen = self.compose(playing, exited, over);
        screen.targets = view::toolbar_targets(&screen.status, &[playing, exited, over]);
        screen.hover = self.hover.filter(|&i| i < screen.targets.len());
        screen
    }

    fn compose(&self, playing: &[Hint], exited: &[Hint], over: &[Hint]) -> view::Screen {
        let frame = self.animation.front();
        let moment = self.moment();
        let game = &moment.game;
        let mut extras = view::Extras {
            par: self.par,
            door_overrides: frame.map_or(&[], |f| f.door_overrides.as_slice()),
            replay: None,
            title: frame.and_then(|f| f.title.as_deref()),
            legend: self.legend,
            lived: None,
            collapse: frame.and_then(|f| f.collapse),
            scroll: Some(self.scroll),
        };
        let separator = || view::colored(" · ", view::Color::Hint);

        let at_end = self.at + 1 == self.tape.len();
        if let Some(paradox) = moment.paradox {
            extras.collapse = extras.collapse.or(Some(COLLAPSED));
            let mut screen = view::screen_with(game, playing, exited, &extras);
            let reason = match paradox {
                Paradox::BrokePast => "✗ paradox: you closed a door your past self walked through".to_string(),
                Paradox::NoWayOut => format!("✗ paradox: no consistent way out within {SEARCH_MOVES} moves"),
            };
            screen.status = view::colored(&reason, view::Color::Key);
            if !over.is_empty() {
                screen.status.extend(separator());
                screen.status.extend(view::hint_glyphs(over));
            }
            return screen;
        }

        if at_end && game.status() == Status::Exited {
            // The level is over: the lived history is the whole history.
            let mut screen = view::screen_with(game, playing, exited, &extras);
            screen.status = view::status_line(game, playing, &[], self.par);
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
            return screen;
        }

        // The whole history, seen from this moment: its self in control, what's on the tape
        // solid, and the rest (predicted from the tape's last moment) dashed.
        let segments = game.segments();
        let controlled = segments.len() - 1;
        extras.replay = Some(Replay { t: game.now(), controlled });
        if self.last().prediction.is_some() {
            let committed = self.last().game.segments();
            extras.lived = Some((committed.len(), committed[committed.len() - 1].path.len()));
        }
        let mut screen = view::screen_with(self.history(), playing, exited, &extras);
        screen.status = view::status_line(game, playing, exited, self.par);
        screen
    }
}

/// The moment of `game`: the rest of history solved, preferring (among equally short ones) a
/// history that puts your other selves at `wanted` now.
fn predicted(game: Game, wanted: &[Pos]) -> Moment {
    let found = if game.status() == Status::Exited {
        return Moment { game, prediction: None, paradox: None };
    } else if game.is_consistent() {
        crate::solver::completion_toward(&game, SEARCH_MOVES, wanted).ok_or(Paradox::NoWayOut)
    } else {
        Err(Paradox::BrokePast)
    };
    match found {
        Ok(moves) => {
            let mut history = game.clone();
            for dir in moves {
                history.step(dir);
            }
            Moment { game, prediction: Some(history), paradox: None }
        }
        Err(paradox) => Moment { game, prediction: None, paradox: Some(paradox) },
    }
}

/// Where your other selves still to come (as `shown` from `before`) would be in `after` if each
/// mirrored your move, as your reflection across the line between you: step along that line and
/// they step the same way, step toward or away and they do too. The prediction prefers this when
/// it has a choice, so wandering doesn't make them shuffle back and forth.
fn mirrored(before: &Game, shown: Option<&Game>, after: &Game) -> Vec<Pos> {
    let (Some(shown), from, to) = (shown, before.player(), after.player()) else {
        return Vec::new();
    };
    if after.now() == before.now() {
        return Vec::new(); // crossing the mirror takes no time: nobody else moves
    }
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    shown.segments()[before.segments().len()..]
        .iter()
        .filter_map(|segment| segment.pos_at(before.now()))
        .map(|other| {
            let (sx, sy) = (other.x - from.x, other.y - from.y);
            if sx.abs() >= sy.abs() {
                Pos::new(other.x - dx, other.y + dy)
            } else {
                Pos::new(other.x + dx, other.y - dy)
            }
        })
        .collect()
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
    fn the_tape_rewinds_and_moving_cuts_it() {
        let mut s = session();
        play(&mut s, "ddwwddddd");
        assert_eq!(s.game().moves(), 9);
        s.input(Input::StepBack);
        s.input(Input::StepBack);
        assert_eq!((s.game().moves(), s.game().now()), (7, 7));
        s.input(Input::StepForward);
        assert_eq!(s.game().moves(), 8); // the rest is still there
        s.input(Input::StepBack);
        s.input(Input::StepBack);
        play(&mut s, "d"); // moving from a rewound moment cuts the tape there
        assert_eq!((s.game().moves(), s.tape.len()), (7, 8));
        s.input(Input::StepForward);
        assert_eq!(s.game().moves(), 7);
        s.input(Input::Restart); // starting over erases the tape
        s.input(Input::StepForward);
        assert_eq!((s.game().moves(), s.tape.len()), (0, 1));
    }

    #[test]
    fn rewinding_keeps_what_is_on_the_tape_committed() {
        let mut s = session();
        play(&mut s, "ddwwdddddddd"); // through the turnstile, inverted
        let lanes = |s: &Session| plain_rows(&s.screen(&[], &[], &[]).timeline)[2..].to_vec();
        let end = lanes(&s);
        for _ in 0..8 {
            s.input(Input::StepBack);
        }
        let rewound = lanes(&s);
        // The same lanes, solid and dashed alike; only where the selves are has changed.
        let is_self = |c: char| "●○◆◇".contains(c);
        assert_eq!(end.len(), rewound.len());
        for (a, b) in end.iter().zip(&rewound) {
            for (x, y) in a.chars().zip(b.chars()) {
                assert!(x == y || is_self(x) || is_self(y), "{end:?} vs {rewound:?}");
            }
        }
    }

    #[test]
    fn the_tape_plays_during_the_game() {
        let mut s = session();
        play(&mut s, "ddw");
        s.settle();
        s.input(Input::Rewind);
        while s.next_deadline().is_some() {
            s.advance();
        }
        assert_eq!(s.game().moves(), 0); // stops at the start
        s.input(Input::Play);
        s.advance();
        assert_eq!(s.game().moves(), 1);
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
    fn doors_shown_closed_dont_slide_again() {
        let mut s = session();
        play(&mut s, "ddwwddddd"); // onto the red trapdoor, the blue one taken by your future self
        s.settle();
        play(&mut s, "d"); // across the mirror, onto the blue trapdoor: its door was closed already
        assert_eq!(s.game().arrow(), crate::game::Arrow::Inverted);
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn winning_celebrates_then_rewinds_the_tape() {
        let mut s = session();
        play(&mut s, PERFECT);
        assert_eq!(s.game().status(), Status::Exited);
        assert_eq!(s.playing, None);
        while s.playing.is_none() {
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
        assert_eq!((s.next_deadline(), s.game().status()), (None, Status::Exited));
    }

    #[test]
    fn stepping_back_after_winning_returns_to_the_game() {
        let mut s = session();
        play(&mut s, PERFECT);
        s.input(Input::StepBack);
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
    fn your_other_self_mirrors_you_when_it_can() {
        let mut s = session();
        play(&mut s, "dddww");
        let other = |s: &Session| {
            let prediction = s.history();
            prediction.segments()[s.game().segments().len()..].iter().find_map(|seg| seg.pos_at(s.game().now())).unwrap()
        };
        // Across the glass, pacing north and south: it paces with you instead of against you.
        for ch in "sswwss".chars() {
            let (me, them) = (s.game().player(), other(&s));
            play(&mut s, &ch.to_string());
            let (me2, them2) = (s.game().player(), other(&s));
            assert_eq!((them2.x - them.x, them2.y - them.y), (0, me2.y - me.y), "after {ch}");
        }
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
        s.input(Input::StepBack);
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
        while s.playing.is_none() {
            let status: String = s.screen(&[], &[], &[]).status.iter().map(|g| g.ch).collect();
            seen.push(status.trim_end().to_string());
            s.advance();
        }
        assert!(seen.iter().any(|l| l.ends_with("· s                m")));
        assert!(seen.iter().any(|l| l.ends_with("· strong d  erminism")));
    }

    #[test]
    fn swipes_on_the_board_move_and_the_timeline_scrolls_the_tape() {
        let mut s = session();
        let board = Some(Region::Board);
        s.pointer(Pointer::Down { x: 10.0, y: 5.0, ms: 0.0, region: board });
        s.pointer(Pointer::Up { x: 14.0, y: 5.5, ms: 100.0 });
        assert_eq!(s.game().player(), Pos::new(1, 6)); // swiped east: onto the entry
        assert!(!s.pointer(Pointer::Down { x: 0.0, y: 0.0, ms: 0.0, region: None })); // not ours
        s.pointer(Pointer::Up { x: 0.0, y: 9.0, ms: 100.0 });
        play(&mut s, "dd");
        assert_eq!(s.game().moves(), 3);

        let timeline = Some(Region::Timeline);
        s.pointer(Pointer::Down { x: 10.0, y: 1.0, ms: 0.0, region: timeline });
        s.pointer(Pointer::Move { x: 11.4, y: 1.0, ms: 500.0 });
        assert_eq!(s.game().moves(), 2); // dragged right a cell: a moment earlier
        s.pointer(Pointer::Up { x: 11.4, y: 1.0, ms: 1000.0 });
        assert_eq!((s.game().moves(), s.fling), (2, None)); // let go still: no fling
        s.pointer(Pointer::Down { x: 10.0, y: 1.0, ms: 0.0, region: timeline });
        s.pointer(Pointer::Up { x: 10.2, y: 1.0, ms: 50.0 });
        assert_eq!((s.game().moves(), s.next_deadline()), (2, None)); // a tap does nothing

        // A quick drag left flings on to later moments until the tape ends.
        s.pointer(Pointer::Down { x: 10.0, y: 1.0, ms: 0.0, region: timeline });
        s.pointer(Pointer::Up { x: 9.0, y: 1.0, ms: 40.0 });
        assert!(s.fling.is_some());
        while s.next_deadline().is_some() {
            s.advance();
        }
        assert_eq!(s.game().moves(), 3);

        s.pointer(Pointer::Wheel { notches: -1.5 });
        assert_eq!(s.game().moves(), 2);
        s.pointer(Pointer::Wheel { notches: -0.5 }); // the half notch left over counts
        assert_eq!(s.game().moves(), 1);
    }

    #[test]
    fn the_toolbar_is_clickable() {
        const HINTS: &[Hint] = &[("←↑↓→ wasd", "move"), ("j k l , .", "tape"), ("r", "restart"), ("q esc", "quit")];
        let mut s = session();
        let screen = s.screen(HINTS, &[], &[]);
        let target = |text: &str| {
            let t = screen.targets.iter().find(|t| {
                screen.status[t.range.clone()].iter().map(|g| g.ch).collect::<String>() == text
            });
            t.map(|t| t.input)
        };
        // Every key on its own, or the whole hint when its keys do one thing.
        assert_eq!(target("↑"), Some(Input::Move(Dir::North)));
        assert_eq!(target("d"), Some(Input::Move(Dir::East)));
        assert_eq!(target(","), Some(Input::StepBack));
        assert_eq!(target("r restart"), Some(Input::Restart));
        assert_eq!(target("q esc quit"), Some(Input::Quit));
        assert_eq!(target("move"), None);
        assert_eq!(screen.targets.len(), 8 + 5 + 2);

        // Find "→" on the laid-out screen, hover it and click it.
        let (cols, rows) = (100, 30);
        let (x, y) = screen
            .layout(cols, rows)
            .iter()
            .find_map(|p| p.row.iter().position(|g| g.ch == '→').map(|i| (p.x + i, p.y)))
            .unwrap();
        let region = screen.region_at(cols, rows, x as f32 + 0.5, y as f32 + 0.5);
        assert!(matches!(region, Some(Region::Toolbar { input: Input::Move(Dir::East), .. })));
        s.pointer(Pointer::Hover { region });
        let hovered = s.screen(HINTS, &[], &[]).grid(cols, rows)[y][x];
        assert_eq!((hovered.ch, hovered.bg), ('→', Some(view::Color::Hover)));
        s.pointer(Pointer::Click { region });
        assert_eq!(s.game().moves(), 1);
        s.pointer(Pointer::Hover { region: None });
        assert_eq!(s.screen(HINTS, &[], &[]).grid(cols, rows)[y][x].bg, None);
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
