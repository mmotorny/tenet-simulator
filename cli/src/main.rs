// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Terminal front end.
//!
//! Interactive:  tenet [LEVEL_FILE]
//! Scripted:     tenet --moves ddddw [LEVEL_FILE]
//!
//! Demo:         tenet --demo-svg [LEVEL_FILE]
//!
//! Moves are w/a/s/d. Scripted mode prints the final frame as text, which makes it easy to
//! discuss a run; with `--svg` it prints it as an SVG image instead. `--demo-svg` records the
//! perfect play, the winning animation and the tape rewinding as one looping animated SVG (the
//! README demo). Colors are on unless `NO_COLOR` is set or `--no-color` is passed.

use std::io::{self, Write};
use std::process::ExitCode;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::{cursor, execute, queue, terminal};
use tenet_core::game::{Dir, Status};
use tenet_core::level::Level;
use tenet_core::session::{Input, Session};
use tenet_core::view::{self, Glyph};

const DEFAULT_LEVEL: &str = tenet_core::levels::FREEPORT;

struct Options {
    level_path: Option<String>,
    moves: Option<String>,
    svg: bool,
    demo_svg: bool,
    color: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut opts = Options {
        level_path: None,
        moves: None,
        svg: false,
        demo_svg: false,
        color: std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--moves" => opts.moves = Some(args.next().ok_or("--moves needs a value")?),
            "--svg" => opts.svg = true,
            "--demo-svg" => opts.demo_svg = true,
            "--no-color" => opts.color = false,
            "-h" | "--help" => return Err(String::new()),
            s if s.starts_with('-') => return Err(format!("unknown option {s}")),
            _ => opts.level_path = Some(arg),
        }
    }
    Ok(opts)
}

const USAGE: &str = "usage: tenet [--moves wasd... [--svg]] [--demo-svg] [--no-color] [LEVEL_FILE]";

fn paint(out: &mut String, text: &str, glyph: &Glyph, enabled: bool) {
    let Glyph { color, bg, bold, .. } = *glyph;
    if !enabled || (color.is_none() && bg.is_none() && !bold) {
        out.push_str(text);
        return;
    }
    if bold {
        out.push_str("\x1b[1m");
    }
    if let Some(c) = color {
        let (r, g, b) = c.rgb();
        out.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
    }
    if let Some(c) = bg {
        let (r, g, b) = c.rgb();
        out.push_str(&format!("\x1b[48;2;{r};{g};{b}m"));
    }
    out.push_str(text);
    out.push_str("\x1b[0m");
}

const PLAYING_HINTS: &[view::Hint] =
    &[("←↑↓→ wasd", "move"), ("u", "undo"), ("r", "restart"), ("?", "legend"), ("q esc", "quit")];
const EXITED_HINTS: &[view::Hint] =
    &[("j", "rewind"), ("k", "pause"), ("l", "play"), (", .", "step"), ("u", "undo"), ("r", "restart"), ("?", "legend"), ("q esc", "quit")];

const PARADOX_HINTS: &[view::Hint] = &[("u", "undo"), ("r", "restart"), ("q esc", "quit")];

fn screen(session: &Session) -> view::Screen {
    session.screen(PLAYING_HINTS, EXITED_HINTS, PARADOX_HINTS)
}

/// A row of glyphs as optionally colored text. Without color, glyphs that rely on color to carry
/// meaning are swapped for distinct shapes.
fn line(row: &[Glyph], color: bool) -> String {
    let mono: Vec<Glyph>;
    let row = if color {
        row
    } else {
        mono = row.iter().copied().map(view::monochrome).collect();
        &mono
    };
    let mut out = String::new();
    // Group runs of the same attributes to keep escape sequences short.
    for run in row.chunk_by(|a, b| (a.color, a.bg, a.bold) == (b.color, b.bg, b.bold)) {
        let text: String = run.iter().map(|g| g.ch).collect();
        paint(&mut out, &text, &run[0], color);
    }
    out
}

/// The screen as one centered column of text, for output that isn't a live terminal screen.
fn stacked_text(session: &Session, with_title: bool, color: bool) -> String {
    screen(session).stacked(with_title).iter().map(|r| line(r, color)).collect::<Vec<_>>().join("\n")
}

fn dir_for_char(ch: char) -> Option<Dir> {
    match ch.to_ascii_lowercase() {
        'w' => Some(Dir::North),
        's' => Some(Dir::South),
        'd' => Some(Dir::East),
        'a' => Some(Dir::West),
        _ => None,
    }
}

fn parse_move(ch: char) -> Result<Dir, String> {
    dir_for_char(ch).ok_or_else(|| format!("invalid move {ch:?}; use w/a/s/d"))
}

/// Plays `moves`, skipping animations; scripted output shows where they end up.
fn play(session: &mut Session, moves: &str) -> Result<(), String> {
    for ch in moves.chars().filter(|c| !c.is_whitespace()) {
        session.input(Input::Move(parse_move(ch)?));
        if session.game().status() == Status::Exited {
            break;
        }
    }
    session.settle();
    Ok(())
}

fn run_scripted(mut session: Session, moves: &str, color: bool) -> Result<(), String> {
    play(&mut session, moves)?;
    // Scripted frames leave out the title to keep runs compact for discussion.
    println!("{}\n", stacked_text(&session, false, color));
    Ok(())
}

/// How long each move stays on screen in the recorded demo.
const DEMO_MOVE_MS: u32 = 350;

/// The README demo: the perfect play of the level with its animations, the winning logo flip and
/// the tape rewinding to the start, as one looping animated SVG.
fn demo_svg(mut session: Session) -> Result<String, String> {
    let moves = tenet_core::solver::shortest_solution(session.game().level(), 80).ok_or("the level has no solution")?;
    let mut frames: Vec<(view::Screen, u32)> = vec![(screen(&session), 1200)];
    for dir in moves {
        session.input(Input::Move(dir));
        // The move's own animations (door slides), then the move itself held for a beat.
        while let Some(ms) = session.next_deadline().filter(|_| session.game().status() != Status::Exited) {
            frames.push((screen(&session), ms));
            session.advance();
        }
        frames.push((screen(&session), DEMO_MOVE_MS));
    }
    // The celebration and the tape rewinding by itself, until it stops at the start.
    frames.pop();
    while let Some(ms) = session.next_deadline() {
        frames.push((screen(&session), ms));
        session.advance();
    }
    frames.push((screen(&session), 2500));

    // One fixed grid for every frame, like a terminal, so nothing shifts between frames.
    let cols = frames.iter().map(|(s, _)| s.min_size().0).max().unwrap_or(0);
    let rows = frames.iter().map(|(s, _)| s.min_size().1).max().unwrap_or(0);
    let grids: Vec<(Vec<Vec<Glyph>>, u32)> = frames.iter().map(|(s, ms)| (s.grid(cols, rows), *ms)).collect();
    Ok(tenet_core::paint::to_animated_svg(&grids))
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
fn draw(session: &Session, color: bool) -> io::Result<()> {
    let (cols, rows) = terminal::size()?;
    let screen = screen(session);
    let mut out = io::stdout();
    queue!(out, terminal::Clear(terminal::ClearType::All))?;
    for view::Placement { x, y, row } in screen.layout(cols as usize, rows as usize) {
        if y < rows as usize {
            queue!(out, cursor::MoveTo(x as u16, y as u16))?;
            write!(out, "{}", line(&row, color))?;
        }
    }
    out.flush()
}

/// The input a key stands for. Movement keys only ever move the protagonist and the tape's
/// transport keys (as in YouTube and video editors) only ever control the tape: no modes.
fn input_for(code: KeyCode) -> Option<Input> {
    Some(match code {
        KeyCode::Up => Input::Move(Dir::North),
        KeyCode::Down => Input::Move(Dir::South),
        KeyCode::Right => Input::Move(Dir::East),
        KeyCode::Left => Input::Move(Dir::West),
        KeyCode::Char('u') => Input::Undo,
        KeyCode::Char('r') => Input::Restart,
        KeyCode::Char('j') => Input::Rewind,
        KeyCode::Char('k') => Input::Pause,
        KeyCode::Char('l') => Input::Play,
        KeyCode::Char(' ') => Input::TogglePlay,
        KeyCode::Char(',') => Input::StepBack,
        KeyCode::Char('.') => Input::StepForward,
        KeyCode::Char('?') => Input::ToggleLegend,
        KeyCode::Char(ch) => Input::Move(dir_for_char(ch)?),
        _ => return None,
    })
}

fn run_interactive(mut session: Session, color: bool) -> io::Result<()> {
    {
        let _guard = TerminalGuard::enter()?;
        loop {
            draw(&session, color)?;
            // Animations and the playing tape move on by themselves when no key arrives in time.
            if let Some(ms) = session.next_deadline() {
                if !event::poll(Duration::from_millis(ms.into()))? {
                    session.advance();
                    continue;
                }
            }
            let Event::Key(KeyEvent { code, modifiers, kind: KeyEventKind::Press, .. }) = event::read()? else {
                continue; // resize and other events just trigger a redraw
            };
            match code {
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char('q') | KeyCode::Esc => break,
                code => {
                    if let Some(input) = input_for(code) {
                        session.input(input);
                    }
                }
            }
        }
    }
    // Leave the final frame on the normal screen.
    println!("{}", stacked_text(&session, false, color));
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
    let session = Session::new(level);
    let result = if opts.demo_svg {
        demo_svg(session).map(|svg| print!("{svg}"))
    } else if opts.svg {
        let mut session = session;
        play(&mut session, opts.moves.as_deref().unwrap_or(""))
            .map(|()| print!("{}", tenet_core::paint::to_svg(&screen(&session).stacked(true))))
    } else if let Some(moves) = &opts.moves {
        run_scripted(session, moves, opts.color)
    } else {
        run_interactive(session, opts.color).map_err(|e| e.to_string())
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

    /// Regenerate with: cargo run -q -- --demo-svg > docs/demo.svg
    #[test]
    fn readme_demo_is_up_to_date() {
        let level = Level::parse(DEFAULT_LEVEL).unwrap();
        let expected = demo_svg(Session::new(level)).unwrap();
        let committed = include_str!("../../docs/demo.svg");
        assert!(committed == expected, "docs/demo.svg is stale; run: cargo run -q -- --demo-svg > docs/demo.svg");
    }
}
