// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Terminal front end.
//!
//! Interactive:  tenet [LEVEL_FILE]
//! Scripted:     tenet --moves ddddw [--trace] [LEVEL_FILE]
//!
//! Moves are w/a/s/d (or h/j/k/l). Scripted mode prints the final frame, or every frame with
//! `--trace`, which makes it easy to discuss a run as plain text. `--svg` prints the final frame
//! as an SVG image instead (used for the README screenshot). Colors are on unless `NO_COLOR`
//! is set or `--no-color` is passed.

use std::io::{self, Write};
use std::process::ExitCode;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, execute, queue, terminal};
use tenet_core::game::{Dir, Game, Status, StepOutcome};
use tenet_core::level::Level;
use tenet_core::view::{self, Color, Glyph};

const DEFAULT_LEVEL: &str = tenet_core::levels::FREEPORT;

struct Options {
    level_path: Option<String>,
    moves: Option<String>,
    trace: bool,
    svg: bool,
    color: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        level_path: None,
        moves: None,
        trace: false,
        svg: false,
        color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--moves" => opts.moves = Some(args.next().ok_or("--moves needs a value")?),
            "--trace" => opts.trace = true,
            "--svg" => opts.svg = true,
            "--no-color" => opts.color = false,
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => opts.level_path = Some(arg),
        }
    }
    Ok(opts)
}

const USAGE: &str = "usage: tenet [--moves wasd...] [--trace | --svg] [--no-color] [LEVEL_FILE]";

fn paint(out: &mut String, text: &str, color: Option<Color>, enabled: bool) {
    match color {
        Some(c) if enabled => {
            let (r, g, b) = c.rgb();
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"));
        }
        _ => out.push_str(text),
    }
}

const PLAYING_HINTS: &[view::Hint] = &[("←↑↓→ wasd hjkl", "move"), ("r", "restart"), ("q", "quit")];
const EXITED_HINTS: &[view::Hint] = &[("r", "restart"), ("q", "quit")];

fn screen(game: &Game) -> view::Screen {
    view::screen(game, PLAYING_HINTS, EXITED_HINTS)
}

/// A row of glyphs as optionally colored text.
fn line(row: &[Glyph], color: bool) -> String {
    let mut out = String::new();
    // Group runs of the same color to keep escape sequences short.
    for run in row.chunk_by(|a, b| a.color == b.color) {
        let text: String = run.iter().map(|g| g.ch).collect();
        paint(&mut out, &text, run[0].color, color);
    }
    out
}

/// The screen as one centered column of text, for output that isn't a live terminal screen.
fn stacked_text(game: &Game, with_title: bool, color: bool) -> String {
    screen(game).stacked(with_title).iter().map(|r| line(r, color)).collect::<Vec<_>>().join("\n")
}

fn dir_for_char(ch: char) -> Option<Dir> {
    match ch.to_ascii_lowercase() {
        'w' | 'k' => Some(Dir::North),
        's' | 'j' => Some(Dir::South),
        'd' | 'l' => Some(Dir::East),
        'a' | 'h' => Some(Dir::West),
        _ => None,
    }
}

fn parse_move(ch: char) -> Result<Dir, String> {
    dir_for_char(ch).ok_or_else(|| format!("invalid move {ch:?}; use w/a/s/d or h/j/k/l"))
}

/// The whole screen, title included, as an SVG image after playing `moves`.
fn svg_frame(mut game: Game, moves: &str) -> Result<String, String> {
    for ch in moves.chars().filter(|c| !c.is_whitespace()) {
        game.step(parse_move(ch)?);
    }
    Ok(view::to_svg(&screen(&game).stacked(true)))
}

fn run_scripted(mut game: Game, moves: &str, trace: bool, color: bool) -> Result<(), String> {
    // Scripted frames leave out the title to keep runs compact for discussion.
    let print = |game: &Game| println!("{}\n", stacked_text(game, false, color));
    if trace {
        print(&game);
    }
    for ch in moves.chars().filter(|c| !c.is_whitespace()) {
        let dir = parse_move(ch)?;
        let outcome = game.step(dir);
        if trace {
            if outcome == StepOutcome::Blocked {
                println!("({ch}: blocked)\n");
            } else {
                print(&game);
            }
        }
        if game.status() == Status::Exited {
            break;
        }
    }
    if !trace {
        print(&game);
    }
    Ok(())
}

/// Restores the terminal even if the game loop returns early or panics.
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// Draws the screen centered: title at the top, status at the bottom, board in between.
fn draw(game: &Game, color: bool) -> io::Result<()> {
    let (cols, rows) = terminal::size()?;
    let (cols, rows) = (cols as usize, rows as usize);
    let screen = screen(game);
    let title_h = screen.title.len();
    let board_h = screen.board.len();

    let mut out = io::stdout();
    queue!(out, terminal::Clear(terminal::ClearType::All))?;
    let mut put = |x: usize, y: usize, row: &[Glyph]| -> io::Result<()> {
        if y < rows {
            queue!(out, cursor::MoveTo(x as u16, y as u16))?;
            write!(out, "{}", line(row, color))?;
        }
        Ok(())
    };
    let centered = |w: usize| cols.saturating_sub(w) / 2;

    let needed = 1 + title_h + 1 + board_h + 1 + 1 + 1;
    let (title_y, status_y) = if rows >= needed { (1, rows - 2) } else { (0, title_h + 1 + board_h + 1) };
    let board_y = {
        let lo = title_y + title_h + 1;
        let hi = status_y.saturating_sub(board_h + 1).max(lo);
        (rows.saturating_sub(board_h) / 2).clamp(lo, hi)
    };

    let title_w = screen.title.first().map_or(0, Vec::len);
    for (i, row) in screen.title.iter().enumerate() {
        put(centered(title_w), title_y + i, row)?;
    }
    for (i, row) in screen.board.iter().enumerate() {
        put(centered(screen.board_width), board_y + i, row)?;
    }
    put(centered(screen.status.len()), status_y, &screen.status)?;
    out.flush()
}

fn run_interactive(level: Level, color: bool) -> io::Result<()> {
    let mut game = Game::new(level.clone());
    {
        let _guard = TerminalGuard::enter()?;
        loop {
            draw(&game, color)?;
            let Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, .. }) = event::read()? else {
                continue; // resize and other events just trigger a redraw
            };
            let dir = match code {
                KeyCode::Up => Some(Dir::North),
                KeyCode::Down => Some(Dir::South),
                KeyCode::Right => Some(Dir::East),
                KeyCode::Left => Some(Dir::West),
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('r') => {
                    game = Game::new(level.clone());
                    None
                }
                KeyCode::Char(ch) => dir_for_char(ch),
                _ => None,
            };
            if let Some(dir) = dir {
                game.step(dir);
            }
        }
    }
    // Leave the final frame on the normal screen.
    println!("{}", stacked_text(&game, false, color));
    Ok(())
}

fn main() -> ExitCode {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("{msg}");
            }
            eprintln!("{USAGE}");
            return if msg.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(2) };
        }
    };

    let text = match &opts.level_path {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("cannot read {path}: {e}");
                return ExitCode::FAILURE;
            }
        },
        None => DEFAULT_LEVEL.to_string(),
    };
    let level = match Level::parse(&text) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("invalid level: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = match (&opts.moves, opts.svg) {
        (moves, true) => svg_frame(Game::new(level), moves.as_deref().unwrap_or("")).map(|svg| print!("{svg}")),
        (Some(moves), false) => run_scripted(Game::new(level), moves, opts.trace, opts.color),
        (None, false) => run_interactive(level, opts.color).map_err(|e| e.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regenerate with: cargo run -q -- --svg > docs/screenshot.svg
    #[test]
    fn readme_screenshot_is_up_to_date() {
        let level = Level::parse(DEFAULT_LEVEL).unwrap();
        let expected = svg_frame(Game::new(level), "").unwrap();
        let committed = include_str!("../../docs/screenshot.svg");
        assert!(committed == expected, "docs/screenshot.svg is stale; run: cargo run -q -- --svg > docs/screenshot.svg");
    }
}
