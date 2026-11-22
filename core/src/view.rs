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
}

/// How a self is drawn: `●` forward, `○` inverted; dimmer for earlier selves replaying.
fn self_glyph(arrow: Arrow, active: bool) -> Glyph {
    match (arrow, active) {
        (Arrow::Forward, true) => Glyph::new('●', Color::Red),
        (Arrow::Forward, false) => Glyph::new('●', Color::RedDim),
        (Arrow::Inverted, true) => Glyph::new('○', Color::Blue),
        (Arrow::Inverted, false) => Glyph::new('○', Color::BlueDim),
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
                    (_, _, true, _) | (_, _, _, true) if !l.n && !l.s => '━',
                    _ => '┃',
                };
                Glyph::new(ch, Color::Wall)
            }
            Some(Tile::Entry) => Glyph::new(DOOR_OPEN, Color::Door),
            Some(Tile::Exit) => {
                let open = self.game.level().exit_open_at(self.game.now());
                Glyph::new(if open { DOOR_OPEN } else { DOOR_FRAMES[DOOR_FRAMES.len() - 1] }, Color::Door)
            }
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
            Some(Tile::TurnstileDoor(_)) => Glyph::new(DOOR_OPEN, self.turnstile_color(x)),
            Some(Tile::Glass) => Glyph::new('⋮', Color::Glass),
            Some(Tile::Floor) => Glyph::new('·', Color::Floor),
            Some(Tile::TurnstileInterior) | None => Glyph::BLANK,
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
    let painter = Painter { game, turnstile_split, selves: game.selves_at(game.now()) };

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

/// The timeline: a global-time axis with a `▼` at the current time, and one swimlane per worldline
/// segment, newest at the bottom. Forward lanes grow rightward, inverted lanes leftward; an
/// inversion connects consecutive lanes with a pair of corners at the moment it happened. Each lane
/// shows where its self is at the current time, if it exists then.
pub fn timeline(game: &Game) -> (Vec<Vec<Glyph>>, usize) {
    let segments = game.segments();
    let (mut lo, mut hi) = game.level().timeline();
    for segment in segments {
        let (a, b) = segment.span();
        lo = lo.min(a - 1);
        hi = hi.max(b + 1);
    }
    let width = (hi - lo + 1) as usize;
    let col = |t: i32| (t - lo) as usize;
    let ticks: Vec<i32> = (lo..=hi).filter(|t| t.rem_euclid(5) == 0).collect();
    let now = game.now();

    let mut labels = vec![Glyph::BLANK; width];
    for &t in &ticks {
        for (i, ch) in t.to_string().chars().enumerate() {
            if let Some(cell) = labels.get_mut(col(t) + i) {
                *cell = Glyph::new(ch, Color::Hint);
            }
        }
    }

    let now_color = match game.arrow() {
        Arrow::Forward => Color::Red,
        Arrow::Inverted => Color::Blue,
    };
    let mut marker = vec![Glyph::BLANK; col(now)];
    marker.extend(text(&format!("▼{now}"), Some(now_color)));

    let mut axis = vec![Glyph::new('─', Color::Hint); width];
    for &t in &ticks {
        axis[col(t)] = Glyph::new('┼', Color::Hint);
    }
    if let Some(t) = game.level().exit_closes().filter(|t| (lo..=hi).contains(t)) {
        axis[col(t)] = Glyph::new(DOOR_FRAMES[DOOR_FRAMES.len() - 1], Color::Door);
    }

    let mut rows = vec![labels, marker, axis];
    let last = segments.len() - 1;
    for (i, segment) in segments.iter().enumerate() {
        let active = i == last;
        let color = match (segment.arrow, active) {
            (Arrow::Forward, true) => Color::Red,
            (Arrow::Forward, false) => Color::RedDim,
            (Arrow::Inverted, true) => Color::Blue,
            (Arrow::Inverted, false) => Color::BlueDim,
        };
        let mut lane = vec![Glyph::BLANK; width];
        let (a, b) = segment.span();
        for t in a..=b {
            lane[col(t)] = Glyph::new('━', color);
        }
        // Corners where this lane connects to the previous and next ones.
        if i > 0 {
            let ch = if segment.arrow == Arrow::Inverted { '┛' } else { '┗' };
            lane[col(segment.start)] = Glyph::new(ch, color);
        }
        if let Some(next) = segments.get(i + 1) {
            let ch = if segment.arrow == Arrow::Forward { '┓' } else { '┏' };
            lane[col(next.start)] = Glyph::new(ch, color);
        }
        if game.selves_at(now).iter().any(|s| s.active == active && segment.pos_at(now) == Some(s.pos)) {
            lane[col(now)] = self_glyph(segment.arrow, active);
        }
        rows.push(lane);
    }
    (rows, width)
}

/// Builds the screen. `playing` hints are shown while playing; `exited` hints follow the message
/// shown after leaving through the exit door.
pub fn screen(game: &Game, playing: &[Hint], exited: &[Hint]) -> Screen {
    let board = render(game);
    let board_width = board.first().map_or(0, Vec::len);
    let (timeline, timeline_width) = timeline(game);
    let status = match game.status() {
        Status::Playing => hints(playing),
        Status::Exited => {
            let message = format!("You walked out of the exit door at t = {}, after {} moves.", game.now(), game.moves());
            let mut line = text(&message, None);
            if !exited.is_empty() {
                line.extend(text(" · ", Some(Color::Hint)));
                line.extend(hints(exited));
            }
            line
        }
    };
    Screen { title: crate::title::logo(), board, board_width, timeline, timeline_width, status }
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

    #[test]
    fn renders_the_bundled_level() {
        let game = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        let expected = "\
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ · · · · · · ⋮ · · · · · · ┃
┃ · · · · · · ⋮ · · · · · · ⠀
┃ · · · · ╭───────╮ · · · · ┃
┃ · · · · ⠀       ⠀ · · · · ┃
┃ · · · · ╰───────╯ · · · · ┃
● · · · · · · ⋮ · · · · · · ┃
┃ · · · · · · ⋮ · · · · · · ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━┛";
        assert_eq!(to_plain_text(&render(&game)), expected);
    }

    #[test]
    fn screen_has_timeline_and_status() {
        let mut game = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        let playing = [("←↑↓→ wasd", "move"), ("q", "quit")];
        let screen = screen(&game, &playing, &[("r", "restart")]);
        assert_eq!((screen.board_width, screen.timeline_width), (29, 25)); // timeline -2..22
        assert_eq!(to_plain_text(std::slice::from_ref(&screen.status)), "←↑↓→ wasd move · q quit");
        let colors: Vec<Option<Color>> = screen.status.iter().map(|g| g.color).collect();
        assert_eq!(colors[0], Some(Color::Key)); // ←
        assert_eq!(colors[4], None); // space between key groups
        assert_eq!(colors[10], Some(Color::Hint)); // "move"

        play(&mut game, "ddddwwwwddaassddddddddd");
        play(&mut game, "wwd");
        let status = to_plain_text(&[screen_of(&game, &playing, &[("r", "restart")]).status]);
        assert_eq!(status, "You walked out of the exit door at t = 8, after 26 moves. · r restart");
    }

    fn screen_of(game: &Game, playing: &[Hint], exited: &[Hint]) -> Screen {
        screen(game, playing, exited)
    }

    fn play(game: &mut Game, moves: &str) {
        use crate::game::Dir::*;
        for ch in moves.chars() {
            game.step(match ch {
                'w' => North,
                's' => South,
                'd' => East,
                'a' => West,
                _ => unreachable!(),
            });
        }
    }

    #[test]
    fn timeline_has_axis_marker_and_one_lane_per_segment() {
        let mut game = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        let (rows, width) = timeline(&game);
        let text = to_plain_text(&rows);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(width, 25);
        assert_eq!(lines[0].trim_end(), "  0    5    10   15   20");
        assert_eq!(lines[1], "  ▼0");
        assert_eq!(lines[2], "──┼────┼───⣿┼────┼────┼──"); // the exit door closes at t = 9
        assert_eq!(lines[3].trim_end(), "  ●");
        assert_eq!(lines.len(), 4);

        // Mid-run: inverted at t = 17, now at t = 14 with the forward self still alive.
        play(&mut game, "ddddwwwwddaassdddddd");
        let (rows, _) = timeline(&game);
        let text = to_plain_text(&rows);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[1], "                ▼14");
        assert_eq!(lines[3].trim_end(), "  ━━━━━━━━━━━━━━●━━┓");
        assert_eq!(lines[4].trim_end(), "                ○━━┛");
        assert_eq!(rows[3][2].color, Some(Color::RedDim)); // the earlier lane is dim
        assert_eq!(rows[4][16].color, Some(Color::Blue)); // the active self is bright
    }

    #[test]
    fn timeline_widens_to_fit_the_worldline() {
        let mut game = Game::new(Level::parse("Er___bX\n\ntimeline: 0..2").unwrap());
        play(&mut game, "dddddd"); // inverted at t = 3, back to t = 0 on the exit
        let (_, width) = timeline(&game);
        assert_eq!(width, 6); // -1..4: one unit of margin around the span 0..3
    }

    #[test]
    fn board_shows_every_self_present_now_and_the_door_by_the_clock() {
        let mut game = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        play(&mut game, "ddddwwwwddaassdddddd"); // t = 14
        let board = render(&game);
        let text = to_plain_text(&board);
        let row = text.lines().nth(4).unwrap();
        assert_eq!(row, "┃ · · · ● ⠀       ⠀ ○ · · · ┃");
        assert_eq!(board[4][8].color, Some(Color::RedDim)); // forward self, replaying
        assert_eq!(board[4][20].color, Some(Color::Blue)); // you
        assert!(text.lines().nth(2).unwrap().ends_with('⣿')); // the exit door is closed at t = 14
    }

    #[test]
    fn stacked_centers_blocks_on_the_map() {
        let game = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        let rows = screen(&game, &[("q", "quit")], &[]).stacked(true);
        let text = to_plain_text(&rows);
        let lines: Vec<&str> = text.lines().collect();
        // Title slab (39 wide) is the widest block: the map is indented by 5, the timeline by 7.
        assert!(lines[0].starts_with("▄▄▄▄"));
        assert!(lines[1].starts_with("  ╺━▄━╸"));
        assert!(lines[6].starts_with("     ┏━━"));
        assert!(lines[16].starts_with("         0    5"));
        assert!(lines[18].starts_with("       ──┼"));
        assert_eq!(lines.last().unwrap().trim(), "q quit");
    }

    #[test]
    fn layout_puts_title_on_top_status_at_bottom_board_and_timeline_centered() {
        let game = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        let screen = screen(&game, &[("q", "quit")], &[]);
        assert_eq!(screen.min_size(), (41, 24));
        let grid = screen.grid(80, 30);
        let text = to_plain_text(&grid);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 30);
        assert!(lines[0].trim().is_empty());
        assert!(lines[1].trim_start().starts_with("▄▄▄"));
        assert_eq!(lines[28].trim(), "q quit");
        let board_top = lines.iter().position(|l| l.contains('┏')).unwrap();
        assert_eq!(board_top, (30 - 14) / 2); // board, blank line and timeline are centered together
        assert_eq!(lines[board_top].find('┏').map(|i| lines[board_top][..i].chars().count()), Some((80 - 29) / 2));
        assert!(lines[board_top + 12].trim_start().starts_with("──┼")); // the axis, under the board
    }

    #[test]
    fn turnstile_halves_are_red_and_blue() {
        let game = Game::new(Level::parse(crate::levels::FREEPORT).unwrap());
        let rows = render(&game);
        assert_eq!(rows[3][10].color, Some(Color::Red)); // ╭
        assert_eq!(rows[3][18].color, Some(Color::Blue)); // ╮
        assert_eq!(rows[4][10].color, Some(Color::Red)); // red door
        assert_eq!(rows[4][18].color, Some(Color::Blue)); // blue door
    }
}
