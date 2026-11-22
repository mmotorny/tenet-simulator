// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Level maps: one character per cell.
//!
//! | Char | Tile |
//! |------|------|
//! | `W`  | wall |
//! | `.`  | floor |
//! | `g`  | glass |
//! | `E`  | entry door |
//! | `X`  | exit door |
//! | `S`  | start: where the protagonist appears, just outside the entry door |
//! | `F`  | finish: stepping onto it ends the level, just outside the exit door |
//! | ` `  | nothing (outside the room) |
//! | `T`  | turnstile frame |
//! | `r`  | turnstile door, red (forward) side |
//! | `b`  | turnstile door, blue (inverted) side |
//! | `_`  | turnstile interior |
//!
//! Rows may be ragged; short rows are padded with nothing. The turnstile is a red door, two
//! interior cells, and a blue door in a straight line. Its mirror lies between the two interior
//! cells: stepping across it reverses a self's arrow of time.
//!
//! After the map, a blank line and optional `key: value` properties:
//!
//! | Key | Meaning |
//! |-----|---------|
//! | `exit-closes` | global time from which the exit door is closed (open before it) |
//! | `timeline` | global time range the timeline shows at least, as `lo..hi` |

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Red,
    Blue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    Void,
    Wall,
    Floor,
    Glass,
    Entry,
    Exit,
    Start,
    Finish,
    TurnstileFrame,
    TurnstileDoor(Side),
    TurnstileInterior,
}

impl Tile {
    fn from_char(ch: char) -> Option<Self> {
        Some(match ch {
            ' ' => Tile::Void,
            'W' => Tile::Wall,
            '.' => Tile::Floor,
            'g' => Tile::Glass,
            'E' => Tile::Entry,
            'X' => Tile::Exit,
            'S' => Tile::Start,
            'F' => Tile::Finish,
            'T' => Tile::TurnstileFrame,
            'r' => Tile::TurnstileDoor(Side::Red),
            'b' => Tile::TurnstileDoor(Side::Blue),
            '_' => Tile::TurnstileInterior,
            _ => return None,
        })
    }

    /// Whether a self can stand here at all. Doors can still be closed at a given time; see
    /// [`crate::game::Game`].
    pub fn is_walkable(self) -> bool {
        matches!(
            self,
            Tile::Floor
                | Tile::Entry
                | Tile::Exit
                | Tile::Start
                | Tile::Finish
                | Tile::TurnstileDoor(_)
                | Tile::TurnstileInterior
        )
    }

    /// Room doors: passing through one is an event on the timeline.
    pub fn is_room_door(self) -> bool {
        matches!(self, Tile::Entry | Tile::Exit)
    }
}

/// The turnstile's cells, red side to blue side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Turnstile {
    pub red_door: Pos,
    pub red_cell: Pos,
    pub blue_cell: Pos,
    pub blue_door: Pos,
}

impl Turnstile {
    pub fn is_door(&self, pos: Pos) -> bool {
        pos == self.red_door || pos == self.blue_door
    }

    pub fn is_inside(&self, pos: Pos) -> bool {
        pos == self.red_cell || pos == self.blue_cell
    }

    /// Whether moving from `from` to `to` crosses the mirror.
    pub fn crosses_mirror(&self, from: Pos, to: Pos) -> bool {
        (from, to) == (self.red_cell, self.blue_cell) || (from, to) == (self.blue_cell, self.red_cell)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    width: i32,
    height: i32,
    tiles: Vec<Tile>,
    start: Pos,
    finish: Pos,
    exit: Pos,
    turnstile: Option<Turnstile>,
    exit_closes: Option<i32>,
    timeline: (i32, i32),
}

/// Timeline range when a level doesn't set one.
const DEFAULT_TIMELINE: (i32, i32) = (0, 20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnknownChar { ch: char, x: usize, y: usize },
    /// A tile that must appear exactly once is missing or repeated.
    NeedsExactlyOne { tile: &'static str, found: usize },
    /// The turnstile needs a red door, two interior cells and a blue door in a straight line.
    BadTurnstile,
    BadProperty { line: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "level map is empty"),
            ParseError::UnknownChar { ch, x, y } => write!(f, "unknown character {ch:?} at ({x}, {y})"),
            ParseError::NeedsExactlyOne { tile, found } => write!(f, "level needs exactly one {tile}, found {found}"),
            ParseError::BadTurnstile => write!(
                f,
                "turnstile needs a red door (r), two interior cells (_) and a blue door (b) in a straight line"
            ),
            ParseError::BadProperty { line } => write!(f, "invalid level property {line:?}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Level {
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut lines = text.lines().map(str::trim_end).skip_while(|l| l.is_empty());
        let rows: Vec<&str> = lines.by_ref().take_while(|l| !l.is_empty()).collect();
        let width = rows.iter().map(|r| r.chars().count()).max().ok_or(ParseError::Empty)?;

        let mut tiles = Vec::with_capacity(width * rows.len());
        let mut found: Vec<(Tile, Pos)> = Vec::new();
        for (y, row) in rows.iter().enumerate() {
            let chars: Vec<char> = row.chars().collect();
            for x in 0..width {
                let ch = chars.get(x).copied().unwrap_or(' ');
                let tile = Tile::from_char(ch).ok_or(ParseError::UnknownChar { ch, x, y })?;
                found.push((tile, Pos::new(x as i32, y as i32)));
                tiles.push(tile);
            }
        }
        let all = |tile: Tile| found.iter().filter(move |(t, _)| *t == tile).map(|&(_, p)| p);
        let one = |tile: Tile, name: &'static str| -> Result<Pos, ParseError> {
            let positions: Vec<Pos> = all(tile).collect();
            match positions.as_slice() {
                [p] => Ok(*p),
                _ => Err(ParseError::NeedsExactlyOne { tile: name, found: positions.len() }),
            }
        };

        let start = one(Tile::Start, "start (S)")?;
        let finish = one(Tile::Finish, "finish (F)")?;
        one(Tile::Entry, "entry door (E)")?;
        let exit = one(Tile::Exit, "exit door (X)")?;
        let mut level = Self {
            width: width as i32,
            height: rows.len() as i32,
            tiles,
            start,
            finish,
            exit,
            turnstile: None,
            exit_closes: None,
            timeline: DEFAULT_TIMELINE,
        };

        let red: Vec<Pos> = all(Tile::TurnstileDoor(Side::Red)).collect();
        let blue: Vec<Pos> = all(Tile::TurnstileDoor(Side::Blue)).collect();
        level.turnstile = match (red.as_slice(), blue.as_slice()) {
            ([], []) => None,
            ([r], [b]) => {
                let (dx, dy) = (b.x - r.x, b.y - r.y);
                let straight = (dx == 0 && dy.abs() == 3) || (dy == 0 && dx.abs() == 3);
                let red_cell = Pos::new(r.x + dx.signum(), r.y + dy.signum());
                let blue_cell = Pos::new(red_cell.x + dx.signum(), red_cell.y + dy.signum());
                let inside = |p| level.tile(p) == Some(Tile::TurnstileInterior);
                if !straight || !inside(red_cell) || !inside(blue_cell) {
                    return Err(ParseError::BadTurnstile);
                }
                Some(Turnstile { red_door: *r, red_cell, blue_cell, blue_door: *b })
            }
            _ => return Err(ParseError::BadTurnstile),
        };

        for line in lines.filter(|l| !l.trim().is_empty()) {
            let bad = || ParseError::BadProperty { line: line.to_string() };
            let (key, value) = line.split_once(':').ok_or_else(bad)?;
            let value = value.trim();
            match key.trim() {
                "exit-closes" => level.exit_closes = Some(value.parse().map_err(|_| bad())?),
                "timeline" => {
                    let (lo, hi) = value.split_once("..").ok_or_else(bad)?;
                    let lo: i32 = lo.trim().parse().map_err(|_| bad())?;
                    let hi: i32 = hi.trim().parse().map_err(|_| bad())?;
                    if lo >= hi {
                        return Err(bad());
                    }
                    level.timeline = (lo, hi);
                }
                _ => return Err(bad()),
            }
        }
        Ok(level)
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// Where the protagonist appears.
    pub fn start(&self) -> Pos {
        self.start
    }

    /// Stepping onto it ends the level.
    pub fn finish(&self) -> Pos {
        self.finish
    }

    pub fn exit(&self) -> Pos {
        self.exit
    }

    pub fn turnstile(&self) -> Option<Turnstile> {
        self.turnstile
    }

    /// Whether the exit door is open at global time `t`.
    pub fn exit_open_at(&self, t: i32) -> bool {
        self.exit_closes.is_none_or(|closes| t < closes)
    }

    /// Global time from which the exit door is closed, if it ever closes.
    pub fn exit_closes(&self) -> Option<i32> {
        self.exit_closes
    }

    /// The global time range the timeline shows at least.
    pub fn timeline(&self) -> (i32, i32) {
        self.timeline
    }

    /// The tile at `pos`, or `None` outside the map.
    pub fn tile(&self, pos: Pos) -> Option<Tile> {
        if (0..self.width).contains(&pos.x) && (0..self.height).contains(&pos.y) {
            Some(self.tiles[(pos.y * self.width + pos.x) as usize])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tiles_and_pads_ragged_rows() {
        let level = Level::parse(" WXF\nSE.W\n WWW\n").unwrap();
        assert_eq!((level.width(), level.height()), (4, 3));
        assert_eq!((level.start(), level.finish(), level.exit()), (Pos::new(0, 1), Pos::new(3, 0), Pos::new(2, 0)));
        assert_eq!(level.tile(Pos::new(0, 0)), Some(Tile::Void));
        assert_eq!(level.tile(Pos::new(2, 1)), Some(Tile::Floor));
        assert_eq!(level.tile(Pos::new(4, 0)), None);
        let ragged = Level::parse("SE.XF\n W").unwrap();
        assert_eq!(ragged.tile(Pos::new(4, 1)), Some(Tile::Void));
    }

    #[test]
    fn rejects_malformed_maps() {
        assert_eq!(Level::parse(""), Err(ParseError::Empty));
        assert_eq!(Level::parse("SE?XF"), Err(ParseError::UnknownChar { ch: '?', x: 2, y: 0 }));
        assert_eq!(Level::parse("E.XF"), Err(ParseError::NeedsExactlyOne { tile: "start (S)", found: 0 }));
        assert_eq!(Level::parse("SSE.XF"), Err(ParseError::NeedsExactlyOne { tile: "start (S)", found: 2 }));
        assert_eq!(Level::parse("SE.X"), Err(ParseError::NeedsExactlyOne { tile: "finish (F)", found: 0 }));
        assert_eq!(Level::parse("S..XF"), Err(ParseError::NeedsExactlyOne { tile: "entry door (E)", found: 0 }));
        assert_eq!(Level::parse("SEr_bXF"), Err(ParseError::BadTurnstile)); // one interior cell
        assert_eq!(Level::parse("SEr___bXF"), Err(ParseError::BadTurnstile)); // three interior cells
        assert_eq!(Level::parse("SEr__XF"), Err(ParseError::BadTurnstile)); // no blue door
        assert!(matches!(Level::parse("SE.XF\n\nfoo: 1"), Err(ParseError::BadProperty { .. })));
        assert!(matches!(Level::parse("SE.XF\n\ntimeline: 5..1"), Err(ParseError::BadProperty { .. })));
    }

    #[test]
    fn parses_properties_and_the_turnstile() {
        let level = Level::parse("SEr__bXF\n\nexit-closes: 9\ntimeline: -2..22\n").unwrap();
        let turnstile = level.turnstile().unwrap();
        assert_eq!(
            turnstile,
            Turnstile {
                red_door: Pos::new(2, 0),
                red_cell: Pos::new(3, 0),
                blue_cell: Pos::new(4, 0),
                blue_door: Pos::new(5, 0)
            }
        );
        assert!(turnstile.crosses_mirror(Pos::new(3, 0), Pos::new(4, 0)));
        assert!(turnstile.crosses_mirror(Pos::new(4, 0), Pos::new(3, 0)));
        assert!(!turnstile.crosses_mirror(Pos::new(2, 0), Pos::new(3, 0)));
        assert!(level.exit_open_at(8) && !level.exit_open_at(9));
        assert_eq!(level.timeline(), (-2, 22));
        let plain = Level::parse("SE.XF").unwrap();
        assert_eq!((plain.turnstile(), plain.exit_open_at(1000), plain.timeline()), (None, true, DEFAULT_TIMELINE));
    }

    #[test]
    fn bundled_level_parses() {
        let level = Level::parse(crate::levels::FREEPORT).unwrap();
        assert_eq!((level.start(), level.finish()), (Pos::new(0, 6), Pos::new(16, 2)));
        assert_eq!(level.exit(), Pos::new(15, 2));
        assert_eq!(level.turnstile().map(|t| (t.red_cell, t.blue_cell)), Some((Pos::new(7, 4), Pos::new(8, 4))));
        assert_eq!(level.exit_closes(), Some(9));
    }
}
