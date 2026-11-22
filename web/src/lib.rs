// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! WebAssembly bindings: the page drives the game through [`WebGame`], which draws the screen onto
//! a canvas the way a terminal emulator draws its cell buffer.

use tenet_core::game::{Dir, Status};
use tenet_core::level::Level;
use tenet_core::levels;
use tenet_core::paint::{paint, Metrics, Shape};
use tenet_core::session::{Input, Session};
use tenet_core::view;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

const PLAYING_HINTS: &[view::Hint] = &[("←↑↓→ wasd swipe", "move"), ("u", "undo"), ("r", "restart"), ("?", "legend")];
const EXITED_HINTS: &[view::Hint] =
    &[("j", "rewind"), ("k", "pause"), ("l", "play"), (", .", "step"), ("u", "undo"), ("r", "restart"), ("?", "legend")];

const PARADOX_HINTS: &[view::Hint] = &[("u", "undo"), ("r", "restart")];

/// JetBrains Mono (loaded by the page), then the GitHub monospace stack as fallback.
const FONT_FAMILY: &str =
    "\"JetBrains Mono\", ui-monospace, SFMono-Regular, \"SF Mono\", Menlo, Consolas, \"Liberation Mono\", monospace";
/// Preferred font size in CSS pixels: the same as GitHub code blocks.
const FONT_PX: f64 = 13.6;
const LINE_HEIGHT: f64 = 1.25;
const BACKGROUND: &str = "#000";

/// The web front end: a thin wrapper around a [`Session`] that maps input names to inputs and
/// draws the session's screen onto a canvas.
#[wasm_bindgen]
pub struct WebGame {
    session: Session,
}

#[wasm_bindgen]
impl WebGame {
    /// Starts the bundled level.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<WebGame, JsError> {
        let level = Level::parse(levels::FREEPORT).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(Self { session: Session::new(level) })
    }

    /// Feeds an input: "north", "south", "east", "west", "undo", "restart", "legend", or one of
    /// the tape's "rewind", "pause", "play", "toggle", "back" and "forward" (after winning).
    pub fn input(&mut self, name: &str) -> Result<(), JsError> {
        let input = match name {
            "north" => Input::Move(Dir::North),
            "south" => Input::Move(Dir::South),
            "east" => Input::Move(Dir::East),
            "west" => Input::Move(Dir::West),
            "undo" => Input::Undo,
            "restart" => Input::Restart,
            "rewind" => Input::Rewind,
            "pause" => Input::Pause,
            "play" => Input::Play,
            "toggle" => Input::TogglePlay,
            "back" => Input::StepBack,
            "forward" => Input::StepForward,
            "legend" => Input::ToggleLegend,
            _ => return Err(JsError::new(&format!("unknown input {name:?}"))),
        };
        self.session.input(input);
        Ok(())
    }

    /// Milliseconds until the screen changes by itself (an animation or the playing tape), or -1
    /// if it waits for input. Call [`WebGame::advance`] then.
    #[wasm_bindgen(js_name = nextDeadline)]
    pub fn next_deadline(&self) -> i32 {
        self.session.next_deadline().map_or(-1, |ms| ms as i32)
    }

    pub fn advance(&mut self) {
        self.session.advance();
    }

    /// Whether the level is over (the tape controls apply).
    pub fn exited(&self) -> bool {
        self.session.game().status() == Status::Exited
    }

    /// Draws the screen onto `canvas`, filling its CSS size at the device's pixel density.
    pub fn render(&self, canvas: &HtmlCanvasElement) -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let dpr = window.device_pixel_ratio();
        let (css_w, css_h) = (canvas.client_width() as f64, canvas.client_height() as f64);
        let (px_w, px_h) = ((css_w * dpr).round() as u32, (css_h * dpr).round() as u32);
        if canvas.width() != px_w || canvas.height() != px_h {
            canvas.set_width(px_w);
            canvas.set_height(px_h);
        }
        let ctx: CanvasRenderingContext2d = canvas.get_context("2d")?.ok_or("no 2d context")?.dyn_into()?;

        ctx.set_font(&format!("100px {FONT_FAMILY}"));
        let advance = ctx.measure_text("0")?.width() / 100.0;
        let screen = self.session.screen(PLAYING_HINTS, EXITED_HINTS, PARADOX_HINTS);
        let grid = Grid::fit(css_w, css_h, dpr, advance, screen.min_size());
        let cells = screen.grid(grid.cols, grid.rows);

        ctx.set_fill_style_str(BACKGROUND);
        ctx.fill_rect(0.0, 0.0, px_w as f64, px_h as f64);
        let font = |bold: bool| format!("{}{}px {FONT_FAMILY}", if bold { "bold " } else { "" }, grid.font_px * dpr);
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        for shape in paint(&cells, &grid.metrics(), grid.ox, grid.oy) {
            match shape {
                Shape::Rect { x, y, w, h, rgb } => {
                    ctx.set_fill_style_str(&css(rgb));
                    ctx.fill_rect(x.into(), y.into(), w.into(), h.into());
                }
                Shape::Line { x1, y1, x2, y2, width, rgb } => {
                    ctx.begin_path();
                    ctx.move_to(x1.into(), y1.into());
                    ctx.line_to(x2.into(), y2.into());
                    ctx.set_line_width(width.into());
                    ctx.set_line_cap("square");
                    ctx.set_stroke_style_str(&css(rgb));
                    ctx.stroke();
                }
                Shape::Arc { cx, cy, rx, ry, start, end, width, rgb } => {
                    ctx.begin_path();
                    ctx.ellipse(cx.into(), cy.into(), rx.into(), ry.into(), 0.0, start.into(), end.into())?;
                    ctx.set_line_width(width.into());
                    ctx.set_line_cap("butt");
                    ctx.set_stroke_style_str(&css(rgb));
                    ctx.stroke();
                }
                Shape::Text { x, y, ch, rgb, bold } => {
                    ctx.set_font(&font(bold));
                    ctx.set_fill_style_str(&css(rgb));
                    let (cx, cy) = (x + grid.metrics().cell_w / 2.0, y + grid.metrics().cell_h / 2.0);
                    ctx.fill_text(ch.encode_utf8(&mut [0; 4]), cx.into(), cy.into())?;
                }
            }
        }
        Ok(())
    }
}

fn css((r, g, b): (u8, u8, u8)) -> String {
    format!("rgb({r},{g},{b})")
}

/// The cell grid for a canvas: font size, whole-device-pixel cell size, grid size, and the
/// offset that centres the grid on the canvas.
#[derive(Debug, PartialEq)]
struct Grid {
    font_px: f64,
    cell_w: f64,
    cell_h: f64,
    cols: usize,
    rows: usize,
    ox: f32,
    oy: f32,
    dpr: f64,
}

impl Grid {
    /// `advance` is the font's character width per pixel of font size. The font shrinks from
    /// [`FONT_PX`] only if the screen's minimum grid wouldn't fit otherwise.
    fn fit(css_w: f64, css_h: f64, dpr: f64, advance: f64, (min_cols, min_rows): (usize, usize)) -> Self {
        let fit = (css_w / (min_cols as f64 * advance)).min(css_h / (min_rows as f64 * LINE_HEIGHT));
        let font_px = FONT_PX.min(fit).max(4.0);
        // Rounded down to whole device pixels, so the grid the font size was chosen for still fits.
        let cell_w = (font_px * advance * dpr).floor().max(1.0);
        let cell_h = (font_px * LINE_HEIGHT * dpr).floor().max(1.0);
        let (px_w, px_h) = ((css_w * dpr).round(), (css_h * dpr).round());
        let (cols, rows) = ((px_w / cell_w).floor() as usize, (px_h / cell_h).floor() as usize);
        let ox = ((px_w - cols as f64 * cell_w) / 2.0).floor() as f32;
        let oy = ((px_h - rows as f64 * cell_h) / 2.0).floor() as f32;
        Self { font_px, cell_w, cell_h, cols, rows, ox, oy, dpr }
    }

    /// Stroke widths in whole device pixels, so painted lines stay crisp.
    fn metrics(&self) -> Metrics {
        let px = |em: f64, min: f64| (self.font_px * em * self.dpr).round().max(min) as f32;
        let light = px(0.08, 1.0);
        Metrics {
            cell_w: self.cell_w as f32,
            cell_h: self.cell_h as f32,
            light,
            heavy: px(0.16, 2.0),
            bold: 4.0 * light,
            bleed: 0.0,
            snap: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADVANCE: f64 = 0.6;

    #[test]
    fn plays_the_level_through_named_inputs() {
        let mut game = WebGame::new().unwrap();
        for ch in "ddwwdddddddddwwdddd".chars() {
            let name = if ch == 'w' { "north" } else { "east" };
            game.input(name).unwrap();
        }
        assert!(game.exited());
        game.input("pause").unwrap(); // skips the celebration; the tape is paused
        assert_eq!(game.next_deadline(), -1);
        game.input("undo").unwrap();
        assert!(!game.exited());
    }

    #[test]
    fn grid_keeps_github_font_size_when_it_fits() {
        let grid = Grid::fit(1000.0, 700.0, 2.0, ADVANCE, (53, 21));
        assert_eq!(grid.font_px, FONT_PX);
        assert_eq!((grid.cell_w, grid.cell_h), (16.0, 34.0)); // 13.6 × 0.6 × 2 and 13.6 × 1.25 × 2, rounded down
        assert_eq!((grid.cols, grid.rows), (125, 41));
        assert!(grid.ox >= 0.0 && grid.oy >= 0.0);
    }

    #[test]
    fn grid_shrinks_the_font_on_narrow_screens() {
        let grid = Grid::fit(300.0, 700.0, 3.0, ADVANCE, (53, 21));
        assert!(grid.font_px < FONT_PX);
        assert!(grid.cols >= 53, "{grid:?}");
    }

    #[test]
    fn strokes_are_whole_pixels_and_bold_is_four_times_light() {
        let m = Grid::fit(1000.0, 700.0, 2.0, ADVANCE, (53, 21)).metrics();
        assert_eq!(m.light, m.light.round());
        assert_eq!(m.bold, 4.0 * m.light);
        assert!(m.snap);
    }
}
