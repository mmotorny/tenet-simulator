// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! WebAssembly bindings: a thin wrapper that lets the page drive the core.

use tenet_core::game::{Dir, Game, Status, StepOutcome};
use tenet_core::level::Level;
use tenet_core::levels;
use tenet_core::view::{self, Color};
use wasm_bindgen::prelude::*;

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

    /// Row of the exit door, where the page shows the time label.
    #[wasm_bindgen(js_name = labelRow)]
    pub fn label_row(&self) -> u32 {
        self.level.exit().y as u32
    }

    /// The map as text, rows separated by newlines.
    pub fn glyphs(&self) -> String {
        view::to_plain_text(&view::render(&self.game))
    }

    /// One color code per glyph, laid out exactly like [`Self::glyphs`]:
    /// `r` red, `b` blue, `g` glass, `w` wall, `f` floor, `d` door, `.` none.
    pub fn colors(&self) -> String {
        view::render(&self.game)
            .iter()
            .map(|row| {
                row.iter()
                    .map(|g| match g.color {
                        Some(Color::Red) => 'r',
                        Some(Color::Blue) => 'b',
                        Some(Color::Glass) => 'g',
                        Some(Color::Wall) => 'w',
                        Some(Color::Floor) => 'f',
                        Some(Color::Door) => 'd',
                        None => '.',
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
    fn glyphs_and_colors_line_up() {
        let game = WebGame::new().unwrap();
        let glyphs: Vec<usize> = game.glyphs().lines().map(|l| l.chars().count()).collect();
        let colors: Vec<usize> = game.colors().lines().map(|l| l.chars().count()).collect();
        assert_eq!(glyphs, colors);
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
