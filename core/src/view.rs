// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Renderer-agnostic view: the level as rows of colored glyphs.
//!
//! Each cell is one glyph, with one separator glyph between neighbouring cells so the map looks
//! roughly square in a terminal. Separators continue horizontal lines (walls, turnstile frame) and
//! are blank otherwise. Front ends only have to map [`Color`] to real colors.

use crate::game::{Arrow, Game, SelfAt, Status};
use crate::level::{Pos, Tile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// Forward in time.
    Red,
    /// Inverted.
    Blue,
    /// Earlier forward selves, replaying.
    RedDim,
    /// Earlier inverted selves, replaying.
    BlueDim,
    Glass,
    Wall,
    Floor,
    Door,
    /// A key or gesture in a status-line hint.
    Key,
    /// The action a hint describes, and the separators between hints.
    Hint,
    /// The title logo's letters: black, as in the film.
    Ink,
    /// The white slab behind the title logo.
    Slab,
}

impl Color {
    /// The palette shared by every front end, as sRGB.
    pub const fn rgb(self) -> (u8, u8, u8) {
        match self {
            Color::Red => (230, 70, 60),
            Color::Blue => (70, 140, 235),
            Color::RedDim => (140, 55, 50),
            Color::BlueDim => (50, 90, 150),
            Color::Glass => (190, 205, 215),
            Color::Wall => (140, 140, 140),
            Color::Floor => (70, 70, 70),
            Color::Door | Color::Key => (230, 230, 230),
            Color::Ink => (12, 12, 12),
            Color::Slab => (240, 240, 240),
            Color::Hint => (110, 110, 110),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    /// Foreground color; `None` is the front end's default text color.
    pub color: Option<Color>,
    /// Background color; `None` is the front end's default background.
    pub bg: Option<Color>,
    /// Drawn heavier: bold text, or a heavy stroke for line glyphs that have no heavy variant
    /// in Unicode (diagonals).
    pub bold: bool,
}

impl Glyph {
    pub const BLANK: Glyph = Glyph { ch: ' ', color: None, bg: None, bold: false };

    pub const fn new(ch: char, color: Color) -> Self {
        Self { ch, color: Some(color), bg: None, bold: false }
    }

    pub const fn plain(ch: char, color: Option<Color>) -> Self {
        Self { ch, color, bg: None, bold: false }
    }

    pub const fn on(ch: char, color: Option<Color>, bg: Color) -> Self {
        Self { ch, color, bg: Some(bg), bold: false }
    }

    pub const fn bold(self) -> Self {
        Self { bold: true, ..self }
    }
}

pub const DOOR_OPEN: char = '⠀';
pub const DOOR_CLOSED: char = '⣿';
/// Where the protagonist starts, outside the entry door; also the start of the first swimlane.
pub const START: char = '▶';
/// Stepping onto it ends the level, outside the exit door; also the end of the last swimlane.
pub const FINISH: char = '■';
/// On a swimlane: passing through a room door.
pub const DOOR_PASSAGE: char = '╋';
/// Door slide animation, open → closed. Used once doors can close.
pub const DOOR_FRAMES: [char; 5] = ['⠀', '⠉', '⠛', '⠿', '⣿'];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Wall,
    Turnstile,
}

fn family(tile: Option<Tile>) -> Option<Family> {
    match tile? {
        Tile::Wall | Tile::Entry | Tile::Exit => Some(Family::Wall),
        Tile::TurnstileFrame | Tile::TurnstileDoor(_) => Some(Family::Turnstile),
        _ => None,
    }
}

/// Which neighbours (N, S, E, W) continue this cell's line.
#[derive(Clone, Copy, Default)]
struct Links {
    n: bool,
    s: bool,
    e: bool,
    w: bool,
}

/// Rendering context for one frame.
struct Painter<'a> {
    game: &'a Game,
    /// Turnstile cells at or left of this column are drawn red, the rest blue.
    turnstile_split: i32,
    /// Every self present now; the active one is drawn on top.
    selves: Vec<SelfAt>,
    /// The global time drawn (the current time, or a moment of the replay).
    now: i32,
    /// Door glyphs to draw instead of the doors' current state, for animation frames.
    door_overrides: &'a [(Pos, char)],
}

/// Glyph substitutions for output without color, where red and blue can't show a self's arrow
/// of time: inverted selves become diamonds (`◆` controlled, `◇` other); forward selves stay
/// circles.
pub fn monochrome(glyph: Glyph) -> Glyph {
    match (glyph.ch, glyph.color) {
        ('●', Some(Color::Blue)) => Glyph { ch: '◆', ..glyph },
        ('○', Some(Color::Blue)) => Glyph { ch: '◇', ..glyph },
        _ => glyph,
    }
}

/// How a self is drawn: filled `●` for the self you control, hollow `○` for every other self
/// (the frontier of your consciousness); red forward, blue inverted.
fn self_glyph(arrow: Arrow, active: bool) -> Glyph {
    let ch = if active { '●' } else { '○' };
    match arrow {
        Arrow::Forward => Glyph::new(ch, Color::Red),
        Arrow::Inverted => Glyph::new(ch, Color::Blue),
    }
}

impl Painter<'_> {
    fn tile(&self, x: i32, y: i32) -> Option<Tile> {
        self.game.level().tile(Pos::new(x, y))
    }

    fn links(&self, x: i32, y: i32) -> Links {
        let Some(fam) = family(self.tile(x, y)) else { return Links::default() };
        let same = |dx: i32, dy: i32| family(self.tile(x + dx, y + dy)) == Some(fam);
        Links { n: same(0, -1), s: same(0, 1), e: same(1, 0), w: same(-1, 0) }
    }

    fn door_glyph(&self, pos: Pos, open: bool, open_glyph: char) -> char {
        let overridden = self.door_overrides.iter().find(|(p, _)| *p == pos).map(|&(_, ch)| ch);
        overridden.unwrap_or(if open { open_glyph } else { DOOR_CLOSED })
    }

    fn turnstile_color(&self, x: i32) -> Color {
        if x <= self.turnstile_split {
            Color::Red
        } else {
            Color::Blue
        }
    }

    fn cell(&self, x: i32, y: i32) -> Glyph {
        let here = Pos::new(x, y);
        if let Some(s) = self.selves.iter().filter(|s| s.pos == here).max_by_key(|s| s.active) {
            return self_glyph(s.arrow, s.active);
        }
        let l = self.links(x, y);
        match self.tile(x, y) {
            Some(Tile::Wall) => {
                let ch = match (l.n, l.s, l.e, l.w) {
                    (false, true, true, false) => '┏',
                    (false, true, false, true) => '┓',
                    (true, false, true, false) => '┗',
                    (true, false, false, true) => '┛',
                    (true, true, true, false) => '┣',
                    (true, true, false, true) => '┫',
                    (false, true, true, true) => '┳',
                    (true, false, true, true) => '┻',
                    (true, true, true, true) => '╋',
                    (_, _, true, _) | (_, _, _, true) if !l.n && !l.s => '━',
                    _ => '┃',
                };
                Glyph::new(ch, Color::Wall)
            }
            Some(Tile::Entry) => {
                let open = self.game.level().entry_open_at(self.now);
                Glyph::new(self.door_glyph(here, open, DOOR_OPEN), Color::Door)
            }
            Some(Tile::Exit) => {
                let open = self.game.level().exit_open_at(self.now);
                Glyph::new(self.door_glyph(here, open, DOOR_OPEN), Color::Door)
            }
            Some(Tile::Start) => Glyph::new(START, Color::Key),
            Some(Tile::Finish) => Glyph::new(FINISH, Color::Key),
            Some(Tile::TurnstileFrame) => {
                let ch = match (l.n, l.s, l.e, l.w) {
                    (false, true, true, false) => '╭',
                    (false, true, false, true) => '╮',
                    (true, false, true, false) => '╰',
                    (true, false, false, true) => '╯',
                    (true, true, false, false) => '│',
                    _ => '─',
                };
                Glyph::new(ch, self.turnstile_color(x))
            }
            Some(Tile::TurnstileDoor(side)) => {
                let open = self.game.turnstile_door_open_at(side, self.now);
                Glyph::new(self.door_glyph(here, open, DOOR_OPEN), self.turnstile_color(x))
            }
            Some(Tile::Glass) => Glyph::new('⋮', Color::Glass),
            Some(Tile::Floor) => Glyph::new('·', Color::Floor),
            Some(Tile::TurnstileInterior | Tile::Void) | None => Glyph::BLANK,
        }
    }

    fn separator(&self, x: i32, y: i32) -> Glyph {
        if !(self.links(x, y).e && self.links(x + 1, y).w) {
            return Glyph::BLANK;
        }
        match family(self.tile(x, y)) {
            Some(Family::Wall) => Glyph::new('━', Color::Wall),
            _ => Glyph::new('─', self.turnstile_color(x + 1)),
        }
    }
}

/// Renders the current game state as rows of `2 * width - 1` glyphs.
pub fn render(game: &Game) -> Vec<Vec<Glyph>> {
    render_with(game, &[])
}

/// Like [`render`], drawing the given doors with the given glyphs (animation frames).
pub fn render_with(game: &Game, door_overrides: &[(Pos, char)]) -> Vec<Vec<Glyph>> {
    render_frame(game, &Extras { door_overrides, ..Extras::default() })
}

/// Renders one frame of the board with its extras.
fn render_frame(game: &Game, extras: &Extras) -> Vec<Vec<Glyph>> {
    let (now, controlled) = match extras.replay {
        Some(r) => (r.t, r.controlled),
        None => (game.now(), game.segments().len() - 1),
    };
    let selves = game.selves_controlled(now, controlled);
    let level = game.level();
    let turnstile_cols: Vec<i32> = (0..level.height())
        .flat_map(|y| (0..level.width()).map(move |x| (x, y)))
        .filter(|&(x, y)| family(level.tile(Pos::new(x, y))) == Some(Family::Turnstile))
        .map(|(x, _)| x)
        .collect();
    let turnstile_split = match (turnstile_cols.iter().min(), turnstile_cols.iter().max()) {
        (Some(lo), Some(hi)) => (lo + hi) / 2,
        _ => 0,
    };
    let painter = Painter {
        game,
        turnstile_split,
        selves,
        now,
        door_overrides: extras.door_overrides,
    };

    (0..level.height())
        .map(|y| {
            let mut row = Vec::with_capacity((level.width() * 2 - 1) as usize);
            for x in 0..level.width() {
                row.push(painter.cell(x, y));
                if x + 1 < level.width() {
                    row.push(painter.separator(x, y));
                }
            }
            row
        })
        .collect()
}

fn text(s: &str, color: Option<Color>) -> Vec<Glyph> {
    s.chars().map(|ch| Glyph::plain(ch, color)).collect()
}

/// One status-line hint: keys (space-separated groups, e.g. `"←↑↓→ wasd"`) and what they do.
pub type Hint<'a> = (&'a str, &'a str);

fn hints(list: &[Hint]) -> Vec<Glyph> {
    let mut out = Vec::new();
    for (i, (keys, action)) in list.iter().enumerate() {
        if i > 0 {
            out.extend(text(" · ", Some(Color::Hint)));
        }
        out.extend(keys.chars().map(|ch| Glyph::plain(ch, (ch != ' ').then_some(Color::Key))));
        out.extend(text(" ", None));
        out.extend(text(action, Some(Color::Hint)));
    }
    out
}

/// Everything a front end lays out: the title at the top, the board and the timeline in the
/// middle, and the status line (hint bar) at the bottom.
pub struct Screen {
    pub title: Vec<Vec<Glyph>>,
    pub board: Vec<Vec<Glyph>>,
    pub board_width: usize,
    pub timeline: Vec<Vec<Glyph>>,
    /// Width of the time axis. Front ends center the timeline on this; the current-time label
    /// next to the `▼` may hang past it.
    pub timeline_width: usize,
    pub status: Vec<Glyph>,
}

/// The timeline: a global-time axis, with the current time labelled among the ticks, and one
/// swimlane per worldline segment, newest at the bottom. Forward lanes grow rightward, inverted
/// lanes leftward; an inversion connects consecutive lanes with a pair of corners at the moment it
/// happened. Lanes also mark the level's start and finish and every passage through a room door,
/// and show where each self is at the current time, if it exists then.
pub fn timeline(game: &Game) -> (Vec<Vec<Glyph>>, usize) {
    timeline_at(game, None)
}

/// The timeline, marking a moment of the replay instead of the current time if given.
fn timeline_at(game: &Game, replay: Option<Replay>) -> (Vec<Vec<Glyph>>, usize) {
    let level = game.level();
    let segments = game.segments();
    let (mut lo, mut hi) = level.timeline();
    for segment in segments {
        let (a, b) = segment.span();
        lo = lo.min(a - 1);
        hi = hi.max(b + 1);
    }
    let width = (hi - lo + 1) as usize;
    let col = |t: i32| (t - lo) as usize;
    let ticks: Vec<i32> = (lo..=hi).filter(|t| t.rem_euclid(5) == 0).collect();
    let (now, controlled) = match replay {
        Some(r) => (r.t, r.controlled),
        None => (game.now(), segments.len() - 1),
    };
    let now_color = match segments[controlled].arrow {
        Arrow::Forward => Color::Red,
        Arrow::Inverted => Color::Blue,
    };

    // Labels: the current time, plus every tick label that keeps at least a cell of space to it.
    let now_label = now.to_string();
    let now_span = col(now)..col(now) + now_label.len();
    let mut labels = vec![Glyph::BLANK; width];
    for &t in &ticks {
        let label = t.to_string();
        let span = col(t)..col(t) + label.len();
        if span.start <= now_span.end && now_span.start <= span.end {
            continue;
        }
        for (i, ch) in label.chars().enumerate() {
            if let Some(cell) = labels.get_mut(span.start + i) {
                *cell = Glyph::new(ch, Color::Hint);
            }
        }
    }
    for (i, ch) in now_label.chars().enumerate() {
        match labels.get_mut(now_span.start + i) {
            Some(cell) => *cell = Glyph::new(ch, now_color),
            None => labels.push(Glyph::new(ch, now_color)),
        }
    }

    let mut axis = vec![Glyph::new('─', Color::Hint); width];
    for &t in &ticks {
        axis[col(t)] = Glyph::new('┼', Color::Hint);
    }
    for t in [level.entry_closes(), level.exit_closes()].into_iter().flatten().filter(|t| (lo..=hi).contains(t)) {
        axis[col(t)] = Glyph::new(DOOR_CLOSED, Color::Door);
    }
    axis[col(now)] = Glyph::new('┼', now_color);

    let mut rows = vec![labels, axis];
    let selves_now = game.selves_controlled(now, controlled);
    for (i, segment) in segments.iter().enumerate() {
        let active = i == controlled;
        let color = match (segment.arrow, active) {
            (Arrow::Forward, true) => Color::Red,
            (Arrow::Forward, false) => Color::RedDim,
            (Arrow::Inverted, true) => Color::Blue,
            (Arrow::Inverted, false) => Color::BlueDim,
        };
        let mut lane = vec![Glyph::BLANK; width];
        for (step, pos) in segment.path.iter().enumerate() {
            let t = segment.start + step as i32 * segment.arrow.dt();
            let first = i == 0 && step == 0;
            let finished = i + 1 == segments.len() && step + 1 == segment.path.len() && game.status() == Status::Exited;
            lane[col(t)] = if first {
                Glyph::new(START, Color::Key)
            } else if finished {
                Glyph::new(FINISH, Color::Key)
            } else if level.tile(*pos).is_some_and(Tile::is_room_door) {
                Glyph::new(DOOR_PASSAGE, Color::Door)
            } else {
                Glyph::new('━', color)
            };
        }
        // Corners where this lane connects to the previous and next ones, both drawn at the moment
        // the self reached the mirror (the crossing itself is a step in the new direction).
        if let Some(prev) = i.checked_sub(1).map(|p| &segments[p]) {
            let ch = if segment.arrow == Arrow::Inverted { '┛' } else { '┗' };
            lane[col(prev.end())] = Glyph::new(ch, color);
        }
        if segments.get(i + 1).is_some() {
            let ch = if segment.arrow == Arrow::Forward { '┓' } else { '┏' };
            lane[col(segment.end())] = Glyph::new(ch, color);
        }
        let over = active && game.status() == Status::Exited && replay.is_none(); // ends with the finish
        let here = segment.pos_at(now);
        if !over && selves_now.iter().any(|s| Some(s.pos) == here && s.arrow == segment.arrow) {
            lane[col(now)] = self_glyph(segment.arrow, active);
        }
        rows.push(lane);
    }
    (rows, width)
}

/// A moment of the replay after winning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Replay {
    pub t: i32,
    /// Index of the segment whose self was being controlled at that moment.
    pub controlled: usize,
}

/// Optional extras for one frame of the screen: the move challenge, and what the session
/// animates (door slides, the logo flip, the tape).
#[derive(Clone, Copy, Default)]
pub struct Extras<'a> {
    /// The fewest moves that finish the level, shown as `moves/par`.
    pub par: Option<u32>,
    /// Doors drawn with these glyphs instead of their state (door slide frames).
    pub door_overrides: &'a [(Pos, char)],
    /// Show this moment of the replay instead of the current one: a global time and the segment
    /// whose self held the frontier of consciousness then.
    pub replay: Option<Replay>,
    /// Draw these rows instead of the title logo (the logo flip).
    pub title: Option<&'a [Vec<Glyph>]>,
    /// Show the legend in place of the timeline.
    pub legend: bool,
}

/// The legend for the map: what its symbols mean, in two columns, each symbol in its real colors.
/// Selves are always explained; other symbols only if they're on the board.
pub fn legend(board: &[Vec<Glyph>]) -> (Vec<Vec<Glyph>>, usize) {
    let on_board = |ch: char| board.iter().flatten().any(|g| g.ch == ch);
    let symbol = |pairs: &[(char, Color)]| pairs.iter().map(|&(ch, c)| Glyph::new(ch, c)).collect::<Vec<_>>();
    // Every self is you, at some point of your own time: the filled one is the self you control.
    // Each self entry shows both arrows of time: red forward, blue inverted.
    let mut entries: Vec<(Vec<Glyph>, &str)> = vec![
        (symbol(&[('●', Color::Red), (' ', Color::Hint), ('●', Color::Blue)]), "you now"),
        (symbol(&[('○', Color::Red), (' ', Color::Hint), ('○', Color::Blue)]), "you another time"),
    ];
    if on_board('⋮') {
        entries.push((symbol(&[('⋮', Color::Glass)]), "glass"));
    }
    if on_board(DOOR_CLOSED) || board.iter().flatten().any(|g| g.color == Some(Color::Door)) {
        entries.push((symbol(&[(DOOR_CLOSED, Color::Door)]), "closed door"));
    }
    if on_board('╭') {
        entries.push((symbol(&[('╭', Color::Red), ('─', Color::Red), ('╮', Color::Blue)]), "turnstile"));
    }
    if on_board(START) || on_board(FINISH) {
        entries.push((symbol(&[(START, Color::Key), (' ', Color::Hint), (FINISH, Color::Key)]), "start, finish"));
    }

    let symbol_width = entries.iter().map(|(sym, _)| sym.len()).max().unwrap_or(0) + 2;
    let cell = |(sym, text): &(Vec<Glyph>, &str)| {
        let mut out = sym.clone();
        out.resize(symbol_width, Glyph::BLANK);
        out.extend(text.chars().map(|ch| Glyph::new(ch, Color::Hint)));
        out
    };
    let cells: Vec<Vec<Glyph>> = entries.iter().map(cell).collect();
    let column = cells.iter().map(Vec::len).max().unwrap_or(0) + 3;
    let rows: Vec<Vec<Glyph>> = cells
        .chunks(2)
        .map(|pair| {
            let mut row = pair[0].clone();
            if let Some(second) = pair.get(1) {
                row.resize(column, Glyph::BLANK);
                row.extend_from_slice(second);
            }
            row
        })
        .collect();
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    (rows, width)
}

/// Builds the screen. `playing` hints are shown while playing; `exited` hints follow the result
/// shown after finishing.
pub fn screen(game: &Game, playing: &[Hint], exited: &[Hint]) -> Screen {
    screen_with(game, playing, exited, &Extras::default())
}

/// The move counter, `moves/par moves` (or `moves moves` without a par), led by a verdict: `✗` as
/// soon as the move count passes par, `✓` on finishing within it.
fn move_counter(game: &Game, par: Option<u32>) -> Vec<Glyph> {
    let missed = par.is_some_and(|par| game.moves() > par);
    let mut line = if missed {
        text("✗ ", Some(Color::Key))
    } else if game.status() == Status::Exited {
        text("✓ ", Some(Color::Key))
    } else {
        Vec::new()
    };
    line.extend(text(&game.moves().to_string(), Some(Color::Key)));
    if let Some(par) = par {
        line.extend(text(&format!("/{par}"), Some(Color::Hint)));
    }
    line.extend(text(" moves", Some(Color::Hint)));
    line
}

/// Like [`screen`], with the extras of one frame.
pub fn screen_with(game: &Game, playing: &[Hint], exited: &[Hint], extras: &Extras) -> Screen {
    let board = render_frame(game, extras);
    let board_width = board.first().map_or(0, Vec::len);
    let (timeline, timeline_width) = if extras.legend { legend(&board) } else { timeline_at(game, extras.replay) };
    let rest = match game.status() {
        Status::Playing => playing,
        Status::Exited => exited,
    };
    let mut status = move_counter(game, extras.par);
    if !rest.is_empty() {
        status.extend(text(" · ", Some(Color::Hint)));
        status.extend(hints(rest));
    }
    let title = extras.title.map_or_else(crate::title::logo, <[Vec<Glyph>]>::to_vec);
    Screen { title, board, board_width, timeline, timeline_width, status }
}

/// A row of glyphs placed at a cell position on a screen.
pub struct Placement<'a> {
    pub x: usize,
    pub y: usize,
    pub row: &'a [Glyph],
}

impl Screen {
    fn title_width(&self) -> usize {
        self.title.first().map_or(0, Vec::len)
    }

    fn timeline_overhang(&self) -> usize {
        self.timeline.iter().map(Vec::len).max().unwrap_or(0).saturating_sub(self.timeline_width)
    }

    /// Height of the middle block: the board, a blank line, and the timeline.
    fn middle_height(&self) -> usize {
        self.board.len() + 1 + self.timeline.len()
    }

    /// The smallest grid, as (columns, rows), that fits the full-screen layout with a one-cell
    /// margin. The timeline is centered on its axis, so its overhanging label needs room on both
    /// sides.
    pub fn min_size(&self) -> (usize, usize) {
        let width = self
            .title_width()
            .max(self.status.len())
            .max(self.board_width)
            .max(self.timeline_width + 2 * self.timeline_overhang());
        (width + 2, 1 + self.title.len() + 1 + self.middle_height() + 1 + 1 + 1)
    }

    /// Lays the screen out on a `cols` × `rows` grid like a full-screen terminal app: title at
    /// the top, status at the bottom, board and timeline centered in between, each centered
    /// horizontally. Rows may extend past the right edge; a front end clips them.
    pub fn layout(&self, cols: usize, rows: usize) -> Vec<Placement<'_>> {
        let (title_h, middle_h) = (self.title.len(), self.middle_height());
        let needed = 1 + title_h + 1 + middle_h + 1 + 1 + 1;
        let (title_y, status_y) = if rows >= needed { (1, rows - 2) } else { (0, title_h + 1 + middle_h + 1) };
        let middle_y = {
            let lo = title_y + title_h + 1;
            let hi = status_y.saturating_sub(middle_h + 1).max(lo);
            (rows.saturating_sub(middle_h) / 2).clamp(lo, hi)
        };
        let timeline_y = middle_y + self.board.len() + 1;
        let centered = |w: usize| cols.saturating_sub(w) / 2;

        let mut placed = Vec::new();
        for (i, row) in self.title.iter().enumerate() {
            placed.push(Placement { x: centered(self.title_width()), y: title_y + i, row });
        }
        for (i, row) in self.board.iter().enumerate() {
            placed.push(Placement { x: centered(self.board_width), y: middle_y + i, row });
        }
        for (i, row) in self.timeline.iter().enumerate() {
            placed.push(Placement { x: centered(self.timeline_width), y: timeline_y + i, row });
        }
        placed.push(Placement { x: centered(self.status.len()), y: status_y, row: &self.status });
        placed
    }

    /// The laid-out screen as a full `cols` × `rows` grid of glyphs, clipped at the edges.
    pub fn grid(&self, cols: usize, rows: usize) -> Vec<Vec<Glyph>> {
        let mut grid = vec![vec![Glyph::BLANK; cols]; rows];
        for Placement { x, y, row } in self.layout(cols, rows) {
            if let Some(line) = grid.get_mut(y) {
                for (i, glyph) in row.iter().enumerate() {
                    if let Some(cell) = line.get_mut(x + i) {
                        *cell = *glyph;
                    }
                }
            }
        }
        grid
    }

    /// The screen as one centered column: title (optional), board, status, separated by blank
    /// lines. Used where there is no fixed-size screen to lay out on, such as scripted output.
    pub fn stacked(&self, with_title: bool) -> Vec<Vec<Glyph>> {
        let title_width = if with_title { self.title_width() } else { 0 };
        let width = title_width.max(self.board_width).max(self.timeline_width).max(self.status.len());
        let pad = |w: usize, row: &[Glyph]| {
            let mut out = vec![Glyph::BLANK; width.saturating_sub(w) / 2];
            out.extend_from_slice(row);
            out
        };
        let mut rows = Vec::new();
        if with_title {
            rows.extend(self.title.iter().map(|r| pad(title_width, r)));
            rows.push(Vec::new());
        }
        rows.extend(self.board.iter().map(|r| pad(self.board_width, r)));
        rows.push(Vec::new());
        rows.extend(self.timeline.iter().map(|r| pad(self.timeline_width, r)));
        rows.push(Vec::new());
        rows.push(pad(self.status.len(), &self.status));
        rows
    }
}

/// Which quadrants of the cell a block-element glyph fills: upper-left, upper-right, lower-left,
/// lower-right. The painter ([`crate::paint`]) draws these as rectangles, the way terminal
/// emulators do, so blocks join seamlessly across cells.
pub fn block_quadrants(ch: char) -> Option<[bool; 4]> {
    Some(match ch {
        '█' => [true, true, true, true],
        '▀' => [true, true, false, false],
        '▄' => [false, false, true, true],
        '▌' => [true, false, true, false],
        '▐' => [false, true, false, true],
        '▘' => [true, false, false, false],
        '▝' => [false, true, false, false],
        '▖' => [false, false, true, false],
        '▗' => [false, false, false, true],
        '▚' => [true, false, false, true],
        '▞' => [false, true, true, false],
        '▛' => [true, true, true, false],
        '▜' => [true, true, false, true],
        '▙' => [true, false, true, true],
        '▟' => [false, true, true, true],
        _ => return None,
    })
}

/// Stroke weights of a box-drawing glyph's arms, toward the left, right, top and bottom edges of
/// the cell: 0 none, 1 light, 2 heavy. The painter ([`crate::paint`]) draws them as bars so lines
/// meet exactly across cells.
pub fn line_arms(ch: char) -> Option<[u8; 4]> {
    Some(match ch {
        '─' => [1, 1, 0, 0],
        '│' => [0, 0, 1, 1],
        '┌' => [0, 1, 0, 1],
        '┐' => [1, 0, 0, 1],
        '└' => [0, 1, 1, 0],
        '┘' => [1, 0, 1, 0],
        '├' => [0, 1, 1, 1],
        '┤' => [1, 0, 1, 1],
        '┬' => [1, 1, 0, 1],
        '┴' => [1, 1, 1, 0],
        '┼' => [1, 1, 1, 1],
        '╴' => [1, 0, 0, 0],
        '╶' => [0, 1, 0, 0],
        '━' => [2, 2, 0, 0],
        '┣' => [0, 2, 2, 2],
        '┫' => [2, 0, 2, 2],
        '┳' => [2, 2, 0, 2],
        '┻' => [2, 2, 2, 0],
        '╋' => [2, 2, 2, 2],
        '╸' => [2, 0, 0, 0],
        '╺' => [0, 2, 0, 0],
        '╹' => [0, 0, 2, 0],
        '╻' => [0, 0, 0, 2],
        '┃' => [0, 0, 2, 2],
        '┏' => [0, 2, 0, 2],
        '┓' => [2, 0, 0, 2],
        '┗' => [0, 2, 2, 0],
        '┛' => [2, 0, 2, 0],
        // Heavy vertical, light horizontal.
        '┎' => [0, 1, 0, 2],
        '┒' => [1, 0, 0, 2],
        '┖' => [0, 1, 2, 0],
        '┚' => [1, 0, 2, 0],
        '┠' => [0, 1, 2, 2],
        '┨' => [1, 0, 2, 2],
        '┰' => [1, 1, 0, 2],
        '┸' => [1, 1, 2, 0],
        _ => return None,
    })
}

/// The rendered view as plain text, one line per row.
pub fn to_plain_text(rows: &[Vec<Glyph>]) -> String {
    rows.iter().map(|row| row.iter().map(|g| g.ch).collect::<String>()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::Level;

    fn freeport() -> Game {
        Game::new(Level::parse(crate::levels::FREEPORT).unwrap())
    }

    fn play(game: &mut Game, moves: &str) {
        use crate::game::Dir::*;
        for ch in moves.chars() {
            game.step(match ch {
                'w' => North,
                's' => South,
                'd' => East,
                _ => West,
            });
        }
    }

    /// Rows as plain text the way a terminal without color shows them, trailing spaces trimmed.
    fn plain(rows: &[Vec<Glyph>]) -> Vec<String> {
        rows.iter().map(|r| r.iter().map(|g| monochrome(*g).ch).collect::<String>().trim_end().to_string()).collect()
    }

    /// The perfect run.
    const PERFECT: &str = "ddwwdddddddddwwdddd";
    /// Just across the mirror, at t = 9 … then one inverted move to t = 8.
    const INVERTED: &str = "ddwwddddddd";

    #[test]
    fn renders_the_bundled_level() {
        let expected = [
            "  ┏━━━━━━━━━━━━━━━━━━━━━━━━━┓",
            "  ┃ · · · · · · ⋮ · · · · · ┃",
            "  ┃ · · · · · · ⋮ · · · · · ⠀ ■",
            "  ┃ · · · ╭─────────╮ · · · ┃",
            "  ┃ · · · ⠀         ⠀ · · · ┃",
            "  ┃ · · · ╰─────────╯ · · · ┃",
            "● ⠀ · · · · · · ⋮ · · · · · ┃",
            "  ┃ · · · · · · ⋮ · · · · · ┃",
            "  ┗━━━━━━━━━━━━━━━━━━━━━━━━━┛",
        ];
        assert_eq!(plain(&render(&freeport())), expected);
    }

    #[test]
    fn turnstile_halves_are_red_and_blue() {
        let rows = render(&freeport());
        assert_eq!(rows[3][10].color, Some(Color::Red)); // ╭
        assert_eq!(rows[3][20].color, Some(Color::Blue)); // ╮
        assert_eq!(rows[4][10].color, Some(Color::Red)); // red door
        assert_eq!(rows[4][20].color, Some(Color::Blue)); // blue door
    }

    #[test]
    fn a_turnstile_door_is_drawn_closed_while_its_trapdoor_is_occupied() {
        let mut game = freeport();
        play(&mut game, "ddwwddddd"); // on the red trapdoor at t = 9
        assert_eq!(plain(&render(&game))[4], "  ┃ · · · ⣿   ●     ⠀ · · · ┃");
    }

    #[test]
    fn board_shows_every_self_present_now_and_the_doors_by_the_clock() {
        let mut game = freeport();
        play(&mut game, INVERTED); // t = 8, inverted
        let board = render(&game);
        let lines = plain(&board);
        assert_eq!(lines[4], "  ┃ · · · ⠀ ○     ◆ ⠀ · · · ┃"); // the forward self, and you (inverted)
        assert_eq!(board[4][12].color, Some(Color::Red)); // hollow: not controlled
        assert_eq!((board[4][18].ch, board[4][18].color), ('●', Some(Color::Blue))); // filled: you
        assert!(lines[2].ends_with("⣿ ■")); // the exit door is closed at t = 8 …
        assert!(lines[6].starts_with("▶ ⣿")); // … and so is the entry door
    }

    #[test]
    fn screen_has_timeline_and_status() {
        let game = freeport();
        let playing = [("←↑↓→ wasd", "move"), ("q", "quit")];
        let screen = screen(&game, &playing, &[("r", "restart")]);
        assert_eq!((screen.board_width, screen.timeline_width), (31, 15)); // timeline −2..12
        assert_eq!(plain(std::slice::from_ref(&screen.status)), ["0 moves · ←↑↓→ wasd move · q quit"]);
        let colors: Vec<Option<Color>> = screen.status.iter().map(|g| g.color).collect();
        assert_eq!(colors[0], Some(Color::Key)); // the move count
        assert_eq!(colors[2], Some(Color::Hint)); // "moves"
        assert_eq!(colors[10], Some(Color::Key)); // ←
    }

    #[test]
    fn the_move_counter_judges_par() {
        let mut game = freeport();
        let extras = Extras { par: Some(19), ..Extras::default() };
        let status = |g: &Game| plain(&[screen_with(g, &[], &[], &extras).status])[0].clone();
        assert_eq!(status(&game), "0/19 moves");
        play(&mut game, PERFECT);
        assert_eq!(status(&game), "✓ 19/19 moves");
        let mut over = freeport();
        play(&mut over, "ddswswswswswswswswswsw"); // wandering: 22 moves without finishing
        assert_eq!(status(&over), "✗ 22/19 moves");
    }

    #[test]
    fn timeline_labels_the_current_time_among_the_ticks_and_has_a_lane_per_segment() {
        let mut game = freeport();
        let (rows, width) = timeline(&game);
        assert_eq!(width, 15);
        assert_eq!(plain(&rows), ["  0    5    10", "──┼──⣿─┼────┼──", "  ●"]);
        assert_eq!(rows[0][2].color, Some(Color::Red)); // the current time, not the tick label

        play(&mut game, INVERTED);
        let (rows, _) = timeline(&game);
        let lines = plain(&rows);
        assert_eq!(lines[0], "  0    5  8 10"); // the current time among the ticks
        assert_eq!(lines[2], "  ▶╋━━━━━━○┓"); // start, entry door … the inversion at t = 9
        assert_eq!(lines[3], "          ◆┛");
        assert_eq!(rows[2][4].color, Some(Color::RedDim)); // the earlier lane is dim
        assert_eq!(rows[3][10].color, Some(Color::Blue)); // yours is bright
    }

    #[test]
    fn timeline_of_the_perfect_run_ends_on_the_finish() {
        let mut game = freeport();
        play(&mut game, PERFECT);
        let (rows, _) = timeline(&game);
        assert_eq!(&plain(&rows)[2..], ["  ○╋━━━━━━━┓", "  ■╋━━━━━━━┛"]);
    }

    #[test]
    fn replay_follows_the_frontier_of_consciousness() {
        let mut game = freeport();
        play(&mut game, PERFECT);
        let at = |t, controlled| {
            let extras = Extras { replay: Some(Replay { t, controlled }), ..Extras::default() };
            let screen = screen_with(&game, &[], &[], &extras);
            (plain(&screen.board)[4].clone(), plain(&screen.timeline)[2..].to_vec())
        };
        // The same moment, t = 8, once as each self.
        assert_eq!(at(8, 1), ("  ┃ · · · ⠀ ○     ◆ ⠀ · · · ┃".into(), vec!["  ▶╋━━━━━━○┓".into(), "  ■╋━━━━━━◆┛".into()]));
        assert_eq!(at(8, 0), ("  ┃ · · · ⠀ ●     ◇ ⠀ · · · ┃".into(), vec!["  ▶╋━━━━━━●┓".into(), "  ■╋━━━━━━◇┛".into()]));
    }

    #[test]
    fn timeline_widens_to_fit_the_worldline() {
        let mut game = Game::new(Level::parse("SEr____bXF\n\ntimeline: 0..2").unwrap());
        play(&mut game, "ddddddddd"); // across the mirror at t = 4, back to t = 0 on the finish
        let (_, width) = timeline(&game);
        assert_eq!(width, 7); // −1..5: one unit of margin around the span 0..4
    }

    #[test]
    fn legend_explains_the_map_in_place_of_the_timeline() {
        let game = freeport();
        let extras = Extras { legend: true, ..Extras::default() };
        let screen = screen_with(&game, &[], &[], &extras);
        assert_eq!(
            plain(&screen.timeline),
            ["● ◆  you now            ○ ◇  you another time", "⋮    glass              ⣿    closed door", "╭─╮  turnstile          ▶ ■  start, finish"]
        );
        assert_eq!(screen.timeline[0][2].color, Some(Color::Blue)); // inverted: blue in color
        // Only symbols on the board are explained.
        let bare = Game::new(Level::parse("SE.XF").unwrap());
        assert_eq!(plain(&legend(&render(&bare)).0), ["● ◆  you now            ○ ◇  you another time", "⣿    closed door        ▶ ■  start, finish"]);
    }

    #[test]
    fn stacked_centers_blocks_on_the_map() {
        let lines = plain(&screen(&freeport(), &[("q", "quit")], &[]).stacked(true));
        // The title slab (39 wide) is the widest block: the map is indented by 4, the timeline by 12.
        assert!(lines[0].starts_with("▄▄▄▄"));
        assert!(lines[1].starts_with("  ╺━▄━╸"));
        assert!(lines[6].starts_with("      ┏━━"));
        assert!(lines[16].starts_with("              0    5"));
        assert!(lines[17].starts_with("            ──┼"));
        assert_eq!(lines.last().unwrap().trim(), "0 moves · q quit");
    }

    #[test]
    fn layout_puts_title_on_top_status_at_bottom_board_and_timeline_centered() {
        let screen = screen(&freeport(), &[("q", "quit")], &[]);
        assert_eq!(screen.min_size(), (41, 23));
        let lines = plain(&screen.grid(80, 30));
        assert_eq!(lines.len(), 30);
        assert!(lines[0].is_empty());
        assert!(lines[1].trim_start().starts_with("▄▄▄"));
        assert_eq!(lines[28].trim(), "0 moves · q quit");
        let board_top = lines.iter().position(|l| l.contains('┏')).unwrap();
        assert_eq!(board_top, (30 - 13) / 2); // board, blank line and timeline are centered together
        assert_eq!(lines[board_top].chars().position(|c| c == '┏'), Some((80 - 31) / 2 + 2));
        assert!(lines[board_top + 11].trim_start().starts_with("──┼")); // the axis, under the board
    }
}

