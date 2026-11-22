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

    /// The title logo, encoded as described in [`encode`].
    pub fn title(&self) -> String {
        encode(&self.screen().title)
    }

    /// The map with the time label, encoded as described in [`encode`].
    pub fn board(&self) -> String {
        encode(&self.screen().board)
    }

    /// Width of the map alone, in cells; the page centers the board on it.
    #[wasm_bindgen(js_name = boardWidth)]
    pub fn board_width(&self) -> usize {
        self.screen().board_width
    }

    /// The hint bar, encoded as described in [`encode`].
    pub fn status(&self) -> String {
        encode(std::slice::from_ref(&self.screen().status))
    }

    fn screen(&self) -> view::Screen {
        view::screen(&self.game, PLAYING_HINTS, EXITED_HINTS)
    }
}

/// Encodes rows for the page: rows are separated by newlines, and each glyph is followed by its
/// color code: `r` red, `b` blue, `g` glass, `w` wall, `f` floor, `d` door, `k` key, `h` hint,
/// `t` title, `.` default.
fn encode(rows: &[Vec<view::Glyph>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .flat_map(|g| {
                    let code = match g.color {
                        Some(Color::Red) => 'r',
                        Some(Color::Blue) => 'b',
                        Some(Color::Glass) => 'g',
                        Some(Color::Wall) => 'w',
                        Some(Color::Floor) => 'f',
                        Some(Color::Door) => 'd',
                        Some(Color::Key) => 'k',
                        Some(Color::Hint) => 'h',
                        Some(Color::Title) => 't',
                        None => '.',
                    };
                    [g.ch, code]
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_each_glyph_with_a_color() {
        let game = WebGame::new().unwrap();
        let board = game.board();
        let rows: Vec<Vec<char>> = board.split('\n').map(|r| r.chars().collect()).collect();
        assert!(rows.iter().all(|r| r.len() % 2 == 0));
        assert_eq!(rows[0][..4], ['┏', 'w', '━', 'w']);
        assert_eq!(game.board_width(), 29);
        let status: String = game.status().chars().step_by(2).collect();
        assert_eq!(status, "←↑↓→ wasd hjkl swipe move · r restart");
        assert_eq!(game.title().split('\n').count(), 4);
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
