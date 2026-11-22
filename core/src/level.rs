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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    RaggedRow { row: usize, expected: usize, found: usize },
    UnknownChar { ch: char, x: usize, y: usize },
    MissingEntry,
    MissingExit,
    MultipleEntries,
    MultipleExits,
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
        }
    }
}

impl std::error::Error for ParseError {}

impl Level {
    pub fn parse(text: &str) -> Result<Self, ParseError> {
        let rows: Vec<&str> = text.lines().map(str::trim_end).filter(|l| !l.is_empty()).collect();
        let width = rows.first().ok_or(ParseError::Empty)?.chars().count();

        let mut tiles = Vec::with_capacity(width * rows.len());
        let (mut entry, mut exit) = (None, None);
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
                    _ => {}
                }
                tiles.push(tile);
            }
        }

        Ok(Self {
            width: width as i32,
            height: rows.len() as i32,
            tiles,
            entry: entry.ok_or(ParseError::MissingEntry)?,
            exit: exit.ok_or(ParseError::MissingExit)?,
        })
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
    }

    #[test]
    fn bundled_level_parses() {
        let level = Level::parse(include_str!("../../levels/freeport.txt")).unwrap();
        assert_eq!(level.entry(), Pos::new(0, 6));
        assert_eq!(level.exit(), Pos::new(14, 2));
    }
}
