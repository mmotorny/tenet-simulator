// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Renderer-agnostic view: the level as rows of colored glyphs.
//!
//! Each cell is one glyph, with one separator glyph between neighbouring cells so the map looks
//! roughly square in a terminal. Separators continue horizontal lines (walls, turnstile frame) and
//! are blank otherwise. Front ends only have to map [`Color`] to real colors.

use crate::game::Game;
use crate::level::{Pos, Tile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// Forward in time.
    Red,
    /// Inverted.
    Blue,
    Glass,
    Wall,
    Floor,
    Door,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Glyph {
    pub ch: char,
    pub color: Option<Color>,
}

impl Glyph {
    const BLANK: Glyph = Glyph { ch: ' ', color: None };

    const fn new(ch: char, color: Color) -> Self {
        Self { ch, color: Some(color) }
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
        if self.game.player() == Pos::new(x, y) {
            return Glyph::new('●', Color::Red);
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
            Some(Tile::Entry | Tile::Exit) => Glyph::new(DOOR_OPEN, Color::Door),
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
    let painter = Painter { game, turnstile_split };

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
        let game = Game::new(Level::parse(include_str!("../../levels/freeport.txt")).unwrap());
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
    fn turnstile_halves_are_red_and_blue() {
        let game = Game::new(Level::parse(include_str!("../../levels/freeport.txt")).unwrap());
        let rows = render(&game);
        assert_eq!(rows[3][10].color, Some(Color::Red)); // ╭
        assert_eq!(rows[3][18].color, Some(Color::Blue)); // ╮
        assert_eq!(rows[4][10].color, Some(Color::Red)); // red door
        assert_eq!(rows[4][18].color, Some(Color::Blue)); // blue door
    }
}
