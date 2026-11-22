// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Terminal front end.
//!
//! Interactive:  tenet [LEVEL_FILE]
//! Scripted:     tenet --moves ddddw [--trace] [LEVEL_FILE]
//!
//! Moves are w/a/s/d (or h/j/k/l). Scripted mode prints the final frame, or every frame with
//! `--trace`, which makes it easy to discuss a run as plain text. Colors are on unless `NO_COLOR`
//! is set or `--no-color` is passed.

use std::io::{self, Write};
use std::process::ExitCode;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, execute, queue, terminal};
use tenet_core::game::{Dir, Game, Status, StepOutcome};
use tenet_core::level::Level;
use tenet_core::view::{self, Color};

const DEFAULT_LEVEL: &str = tenet_core::levels::FREEPORT;

struct Options {
    level_path: Option<String>,
    moves: Option<String>,
    trace: bool,
    color: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        level_path: None,
        moves: None,
        trace: false,
        color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--moves" => opts.moves = Some(args.next().ok_or("--moves needs a value")?),
            "--trace" => opts.trace = true,
            "--no-color" => opts.color = false,
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => opts.level_path = Some(arg),
        }
    }
    Ok(opts)
}

const USAGE: &str = "usage: tenet [--moves wasd...] [--trace] [--no-color] [LEVEL_FILE]";

fn rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Red => (230, 70, 60),
        Color::Blue => (70, 140, 235),
        Color::Glass => (190, 205, 215),
        Color::Wall => (140, 140, 140),
        Color::Floor => (70, 70, 70),
        Color::Door => (230, 230, 230),
    }
}

fn paint(out: &mut String, text: &str, color: Option<Color>, enabled: bool) {
    match color {
        Some(c) if enabled => {
            let (r, g, b) = rgb(c);
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m"));
        }
        _ => out.push_str(text),
    }
}

/// The screen (map, time label, status) as lines of optionally colored text.
fn frame_lines(game: &Game, color: bool) -> Vec<String> {
    view::screen(game, "arrows / wasd / hjkl move · q quit", "")
        .iter()
        .map(|row| {
            let mut line = String::new();
            // Group runs of the same color to keep escape sequences short.
            for run in row.chunk_by(|a, b| a.color == b.color) {
                let text: String = run.iter().map(|g| g.ch).collect();
                paint(&mut line, &text, run[0].color, color);
            }
            line
        })
        .collect()
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

fn run_scripted(mut game: Game, moves: &str, trace: bool, color: bool) -> Result<(), String> {
    let print = |game: &Game| println!("{}\n", frame_lines(game, color).join("\n"));
    if trace {
        print(&game);
    }
    for ch in moves.chars().filter(|c| !c.is_whitespace()) {
        let dir = dir_for_char(ch).ok_or_else(|| format!("invalid move {ch:?}; use w/a/s/d or h/j/k/l"))?;
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

fn draw(game: &Game, color: bool) -> io::Result<()> {
    let mut out = io::stdout();
    queue!(out, cursor::MoveTo(0, 0), terminal::Clear(terminal::ClearType::All))?;
    for line in frame_lines(game, color) {
        write!(out, "{line}\r\n")?;
    }
    out.flush()
}

fn run_interactive(mut game: Game, color: bool) -> io::Result<()> {
    {
        let _guard = TerminalGuard::enter()?;
        loop {
            draw(&game, color)?;
            if game.status() == Status::Exited {
                break;
            }
            let Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, .. }) = event::read()? else {
                continue; // resize and other events just trigger a redraw
            };
            let dir = match code {
                KeyCode::Up => Some(Dir::North),
                KeyCode::Down => Some(Dir::South),
                KeyCode::Right => Some(Dir::East),
                KeyCode::Left => Some(Dir::West),
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok(()),
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char(ch) => dir_for_char(ch),
                _ => None,
            };
            if let Some(dir) = dir {
                game.step(dir);
            }
        }
    }
    // Leave the final frame on the normal screen.
    println!("{}", frame_lines(&game, color).join("\n"));
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
    let game = Game::new(level);

    let result = match &opts.moves {
        Some(moves) => run_scripted(game, moves, opts.trace, opts.color),
        None => run_interactive(game, opts.color).map_err(|e| e.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
