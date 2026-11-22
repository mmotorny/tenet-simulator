// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Level maps: one character per cell.
//!
//! | Char | Tile |
//! |------|------|
//! | `W`  | wall |
//! | `.`  | floor |
//! | `g`  | glass |
//! | `E`  | entry door (where the protagonist appears) |
//! | `X`  | exit door (the goal) |
//! | `T`  | turnstile frame |
//! | `r`  | turnstile door, red (forward) side |
//! | `b`  | turnstile door, blue (inverted) side |
//! | `_`  | turnstile interior |
//!
//! The turnstile's mirror, where a self's arrow of time reverses, is the interior cell halfway
//! between its red and blue doors.
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
    Wall,
    Floor,
    Glass,
    Entry,
    Exit,
    TurnstileFrame,
    TurnstileDoor(Side),
    TurnstileInterior,
}

impl Tile {
    fn from_char(ch: char) -> Option<Self> {
        Some(match ch {
            'W' => Tile::Wall,
            '.' => Tile::Floor,
            'g' => Tile::Glass,
            'E' => Tile::Entry,
            'X' => Tile::Exit,
            'T' => Tile::TurnstileFrame,
            'r' => Tile::TurnstileDoor(Side::Red),
            'b' => Tile::TurnstileDoor(Side::Blue),
            '_' => Tile::TurnstileInterior,
            _ => return None,
        })
    }

    /// Whether the protagonist can stand here. Until time mechanics exist, the turnstile is a
    /// plain passage through the glass wall: in through one door, out through the other.
    pub fn is_walkable(self) -> bool {
        matches!(
            self,
            Tile::Floor | Tile::Entry | Tile::Exit | Tile::TurnstileDoor(_) | Tile::TurnstileInterior
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Level {
    width: i32,
    height: i32,
    tiles: Vec<Tile>,
    entry: Pos,
    exit: Pos,
    mirror: Option<Pos>,
    exit_closes: Option<i32>,
    timeline: (i32, i32),
}

/// Timeline range when a level doesn't set one.
const DEFAULT_TIMELINE: (i32, i32) = (0, 20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    RaggedRow { row: usize, expected: usize, found: usize },
    UnknownChar { ch: char, x: usize, y: usize },
    MissingEntry,
    MissingExit,
    MultipleEntries,
    MultipleExits,
    /// The turnstile needs one red door and one blue door with an odd-length interior between them.
    BadTurnstile,
    BadProperty { line: String },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Empty => write!(f, "level map is empty"),
            ParseError::RaggedRow { row, expected, found } => {
                write!(f, "row {row} has {found} cells, expected {expected}")
            }
            ParseError::UnknownChar { ch, x, y } => write!(f, "unknown character {ch:?} at ({x}, {y})"),
            ParseError::MissingEntry => write!(f, "level has no entry door (E)"),
            ParseError::MissingExit => write!(f, "level has no exit door (X)"),
            ParseError::MultipleEntries => write!(f, "level has more than one entry door (E)"),
            ParseError::MultipleExits => write!(f, "level has more than one exit door (X)"),
            ParseError::BadTurnstile => {
                write!(f, "turnstile needs one red door (r) and one blue door (b) with a middle cell between them")
            }
            ParseError::BadProperty { line } => write!(f, "invalid level property {line:?}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl Level {
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let mut lines = text.lines().map(str::trim_end).skip_while(|l| l.is_empty());
        let rows: Vec<&str> = lines.by_ref().take_while(|l| !l.is_empty()).collect();
        let width = rows.first().ok_or(ParseError::Empty)?.chars().count();

        let mut tiles = Vec::with_capacity(width * rows.len());
        let (mut entry, mut exit) = (None, None);
        let (mut red, mut blue) = (Vec::new(), Vec::new());
        for (y, row) in rows.iter().enumerate() {
            let found = row.chars().count();
            if found != width {
                return Err(ParseError::RaggedRow { row: y, expected: width, found });
            }
            for (x, ch) in row.chars().enumerate() {
                let tile = Tile::from_char(ch).ok_or(ParseError::UnknownChar { ch, x, y })?;
                let pos = Pos::new(x as i32, y as i32);
                match tile {
                    Tile::Entry if entry.replace(pos).is_some() => return Err(ParseError::MultipleEntries),
                    Tile::Exit if exit.replace(pos).is_some() => return Err(ParseError::MultipleExits),
                    Tile::TurnstileDoor(Side::Red) => red.push(pos),
                    Tile::TurnstileDoor(Side::Blue) => blue.push(pos),
                    _ => {}
                }
                tiles.push(tile);
            }
        }

        let mut level = Self {
            width: width as i32,
            height: rows.len() as i32,
            tiles,
            entry: entry.ok_or(ParseError::MissingEntry)?,
            exit: exit.ok_or(ParseError::MissingExit)?,
            mirror: None,
            exit_closes: None,
            timeline: DEFAULT_TIMELINE,
        };
        level.mirror = match (red.as_slice(), blue.as_slice()) {
            ([], []) => None,
            ([r], [b]) => {
                let mid = Pos::new((r.x + b.x) / 2, (r.y + b.y) / 2);
                let aligned = (r.x == b.x || r.y == b.y) && (r.x + b.x) % 2 == 0 && (r.y + b.y) % 2 == 0;
                if !aligned || level.tile(mid) != Some(Tile::TurnstileInterior) {
                    return Err(ParseError::BadTurnstile);
                }
                Some(mid)
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
                    let (lo, hi): (i32, i32) = (lo.trim().parse().map_err(|_| bad())?, hi.trim().parse().map_err(|_| bad())?);
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

    pub fn entry(&self) -> Pos {
        self.entry
    }

    pub fn exit(&self) -> Pos {
        self.exit
    }

    /// The turnstile's mirror: stepping onto it reverses a self's arrow of time.
    pub fn mirror(&self) -> Option<Pos> {
        self.mirror
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
    fn parses_doors_and_tiles() {
        let level = Level::parse("WWXW\nE..W\nWWWW\n").unwrap();
        assert_eq!((level.width(), level.height()), (4, 3));
        assert_eq!(level.entry(), Pos::new(0, 1));
        assert_eq!(level.exit(), Pos::new(2, 0));
        assert_eq!(level.tile(Pos::new(1, 1)), Some(Tile::Floor));
        assert_eq!(level.tile(Pos::new(4, 0)), None);
    }

    #[test]
    fn rejects_malformed_maps() {
        assert_eq!(Level::parse(""), Err(ParseError::Empty));
        assert_eq!(Level::parse("WXW\nE.\n"), Err(ParseError::RaggedRow { row: 1, expected: 3, found: 2 }));
        assert_eq!(Level::parse("E?X"), Err(ParseError::UnknownChar { ch: '?', x: 1, y: 0 }));
        assert_eq!(Level::parse("W.X"), Err(ParseError::MissingEntry));
        assert_eq!(Level::parse("E.W"), Err(ParseError::MissingExit));
        assert_eq!(Level::parse("EEX"), Err(ParseError::MultipleEntries));
        assert_eq!(Level::parse("ErbX"), Err(ParseError::BadTurnstile)); // no middle cell
        assert_eq!(Level::parse("Er__bX"), Err(ParseError::BadTurnstile)); // even interior
        assert_eq!(Level::parse("Er_X"), Err(ParseError::BadTurnstile)); // no blue door
        assert!(matches!(Level::parse("E.X\n\nfoo: 1"), Err(ParseError::BadProperty { .. })));
        assert!(matches!(Level::parse("E.X\n\ntimeline: 5..1"), Err(ParseError::BadProperty { .. })));
    }

    #[test]
    fn parses_properties_and_finds_the_mirror() {
        let level = Level::parse("Er___bX\n\nexit-closes: 9\ntimeline: -2..22\n").unwrap();
        assert_eq!(level.mirror(), Some(Pos::new(3, 0)));
        assert!(level.exit_open_at(8) && !level.exit_open_at(9));
        assert_eq!(level.timeline(), (-2, 22));
        let plain = Level::parse("E.X").unwrap();
        assert_eq!((plain.mirror(), plain.exit_open_at(1000), plain.timeline()), (None, true, DEFAULT_TIMELINE));
    }

    #[test]
    fn bundled_level_parses() {
        let level = Level::parse(crate::levels::FREEPORT).unwrap();
        assert_eq!(level.entry(), Pos::new(0, 6));
        assert_eq!(level.exit(), Pos::new(14, 2));
        assert_eq!(level.mirror(), Some(Pos::new(7, 4)));
        assert_eq!(level.exit_closes(), Some(9));
    }
}
