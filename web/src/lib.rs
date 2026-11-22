// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! WebAssembly bindings: a thin wrapper that lets the page drive the core.

use tenet_core::game::{Dir, Game, Status, StepOutcome};
use tenet_core::level::Level;
use tenet_core::levels;
use tenet_core::view::{self, Color};
use wasm_bindgen::prelude::*;

const PLAYING_HINTS: &[view::Hint] = &[("←↑↓→ wasd hjkl swipe", "move"), ("r", "restart")];
const EXITED_HINTS: &[view::Hint] = &[("r tap", "restart")];

#[wasm_bindgen]
pub struct WebGame {
    level: Level,
    game: Game,
}

#[wasm_bindgen]
impl WebGame {
    /// Starts the bundled prototype level.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WebGame, JsError> {
        let level = Level::parse(levels::FREEPORT).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { game: Game::new(level.clone()), level })
    }

    pub fn reset(&mut self) {
        self.game = Game::new(self.level.clone());
    }

    /// Moves the protagonist: `dir` is "north", "south", "east" or "west".
    /// Returns "moved", "blocked" or "exited".
    pub fn step(&mut self, dir: &str) -> Result<String, JsError> {
        let dir = match dir {
            "north" => Dir::North,
            "south" => Dir::South,
            "east" => Dir::East,
            "west" => Dir::West,
            _ => return Err(JsError::new(&format!("unknown direction {dir:?}"))),
        };
        Ok(match self.game.step(dir) {
            StepOutcome::Moved => "moved",
            StepOutcome::Blocked => "blocked",
            StepOutcome::Exited => "exited",
        }
        .to_string())
    }

    pub fn turn(&self) -> u32 {
        self.game.turn()
    }

    pub fn exited(&self) -> bool {
        self.game.status() == Status::Exited
    }

    /// The title logo: black letters on a white slab.
    pub fn title(&self) -> Cells {
        Cells::new(&self.screen().title)
    }

    /// The map with the time label to the right of the exit row.
    pub fn board(&self) -> Cells {
        Cells::new(&self.screen().board)
    }

    /// Width of the map alone, in cells; the page centers the board on it.
    #[wasm_bindgen(js_name = boardWidth)]
    pub fn board_width(&self) -> usize {
        self.screen().board_width
    }

    /// The hint bar.
    pub fn status(&self) -> Cells {
        Cells::new(std::slice::from_ref(&self.screen().status))
    }

    fn screen(&self) -> view::Screen {
        view::screen(&self.game, PLAYING_HINTS, EXITED_HINTS)
    }
}

/// A rectangle of cells, like a terminal emulator's cell buffer: one glyph and its attributes per
/// cell, in parallel arrays indexed by `y * width + x`. Rows shorter than `width` are padded with
/// blank cells.
#[wasm_bindgen]
pub struct Cells {
    width: usize,
    height: usize,
    chars: Vec<u32>,
    fg: Vec<u8>,
    bg: Vec<u8>,
    bold: Vec<u8>,
}

#[wasm_bindgen]
impl Cells {
    fn new(rows: &[Vec<view::Glyph>]) -> Self {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let height = rows.len();
        let mut cells = Self {
            width,
            height,
            chars: vec![' ' as u32; width * height],
            fg: vec![0; width * height],
            bg: vec![0; width * height],
            bold: vec![0; width * height],
        };
        for (y, row) in rows.iter().enumerate() {
            for (x, glyph) in row.iter().enumerate() {
                let i = y * width + x;
                cells.chars[i] = glyph.ch as u32;
                cells.fg[i] = palette_index(glyph.color);
                cells.bg[i] = palette_index(glyph.bg);
                cells.bold[i] = glyph.bold.into();
            }
        }
        cells
    }

    #[wasm_bindgen(getter)]
    pub fn width(&self) -> usize {
        self.width
    }

    #[wasm_bindgen(getter)]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Unicode code points (a `Uint32Array` in JavaScript).
    #[wasm_bindgen(getter)]
    pub fn chars(&self) -> Vec<u32> {
        self.chars.clone()
    }

    /// Foreground colors as palette indices (a `Uint8Array`); see [`PALETTE`].
    #[wasm_bindgen(getter)]
    pub fn fg(&self) -> Vec<u8> {
        self.fg.clone()
    }

    /// Background colors as palette indices (a `Uint8Array`); see [`PALETTE`].
    #[wasm_bindgen(getter)]
    pub fn bg(&self) -> Vec<u8> {
        self.bg.clone()
    }

    /// 1 where the glyph is bold (a `Uint8Array`).
    #[wasm_bindgen(getter)]
    pub fn bold(&self) -> Vec<u8> {
        self.bold.clone()
    }
}

/// Palette order shared with the page: index 0 is the default color, then these. The page maps
/// each index to a CSS class (`PALETTE_CLASSES` in `www/main.js`).
const PALETTE: [Color; 10] = [
    Color::Red,
    Color::Blue,
    Color::Glass,
    Color::Wall,
    Color::Floor,
    Color::Door,
    Color::Key,
    Color::Hint,
    Color::Ink,
    Color::Slab,
];

fn palette_index(color: Option<Color>) -> u8 {
    color.map_or(0, |c| PALETTE.iter().position(|&p| p == c).expect("color missing from PALETTE") as u8 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_hold_glyphs_and_attributes_in_parallel_arrays() {
        let game = WebGame::new().unwrap();
        let board = game.board();
        assert_eq!(board.height(), 9);
        assert_eq!(board.width(), 40); // the label row is the longest; other rows are padded
        assert_eq!((board.chars().len(), board.fg().len(), board.bg().len()), (360, 360, 360));
        assert_eq!(board.chars()[0], '┏' as u32);
        assert_eq!(board.fg()[0], palette_index(Some(Color::Wall)));
        assert_eq!(board.bg()[0], 0);
        assert_eq!(board.chars()[board.width() - 1], ' ' as u32); // padding
        assert_eq!(game.board_width(), 29);

        let status = game.status();
        let text: String = status.chars().iter().map(|&c| char::from_u32(c).unwrap()).collect();
        assert_eq!(text, "←↑↓→ wasd hjkl swipe move · r restart");

        let title = game.title();
        assert_eq!((title.width(), title.height()), (37, 7));
        assert_eq!(title.chars()[0], '▄' as u32);
        assert_eq!(title.fg()[0], palette_index(Some(Color::Slab)));
        let diagonal = title.chars().iter().position(|&c| c == '╲' as u32).unwrap();
        assert_eq!((title.bold()[diagonal], title.bold()[0]), (1, 0));
    }

    #[test]
    fn palette_indices_are_distinct() {
        let mut indices: Vec<u8> = PALETTE.iter().map(|&c| palette_index(Some(c))).collect();
        indices.push(palette_index(None));
        indices.sort();
        indices.dedup();
        assert_eq!(indices.len(), PALETTE.len() + 1);
    }

    #[test]
    fn walks_out_of_the_exit() {
        let mut game = WebGame::new().unwrap();
        let moves = [("east", 4), ("north", 2), ("east", 9), ("north", 2), ("east", 1)];
        let mut last = String::new();
        for (dir, n) in moves {
            for _ in 0..n {
                last = game.step(dir).unwrap();
            }
        }
        assert_eq!(last, "exited");
        assert!(game.exited());
        game.reset();
        assert_eq!((game.turn(), game.exited()), (0, false));
    }
}
