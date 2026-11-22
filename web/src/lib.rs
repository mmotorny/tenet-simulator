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

    /// The whole screen, exactly as the terminal shows it. Rows are separated by newlines; each
    /// glyph is followed by its color code: `r` red, `b` blue, `g` glass, `w` wall, `f` floor,
    /// `d` door, `k` key, `h` hint, `.` default.
    pub fn screen(&self) -> String {
        view::screen(&self.game, PLAYING_HINTS, EXITED_HINTS)
            .iter()
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
                            None => '.',
                        };
                        [g.ch, code]
                    })
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_pairs_each_glyph_with_a_color() {
        let game = WebGame::new().unwrap();
        let screen = game.screen();
        let rows: Vec<Vec<char>> = screen.split('\n').map(|r| r.chars().collect()).collect();
        assert!(rows.iter().all(|r| r.len() % 2 == 0));
        assert_eq!(rows[0][..4], ['┏', 'w', '━', 'w']);
        let last: String = rows.last().unwrap().iter().step_by(2).collect();
        assert_eq!(last, "←↑↓→ wasd hjkl swipe move · r restart");
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
