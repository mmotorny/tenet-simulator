<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- SPDX-FileCopyrightText: 2026 Maksym Motorny -->

# Tenet Simulator

A puzzle simulator of strong determinism and time inversion, inspired by Christopher Nolan's *Tenet*. The goal is to build intuition for what it feels like to pass through a turnstile, and to solve time puzzles that seem impossible in the real world.

The world is a 2D top-down grid with discrete time and space, played on hand-designed levels. It runs in the terminal and in the browser from one shared Rust core.

**Play in the browser:** https://mmotorny.github.io/tenet-simulator/

[![The perfect play of the first room: your future self waits on the finish from the start; you go in through the west door, through the turnstile, across the mirror and back out through the east door in the past, finishing at t = 0 as the run began, with strong determinism; then the logo flip and the tape of consciousness rewinding](docs/demo.svg)](https://mmotorny.github.io/tenet-simulator/)

## How it plays

You start on `▶` outside the west door and finish by stepping onto `■` outside the east door. Both doors close at t = 3, long before you can reach the east one, and a glass wall splits the room anyway.

The way through is the turnstile. Walk in through its red door and cross the invisible mirror in the middle: you become inverted, and from then on every move takes you one step back in global time. The perfect run is a palindrome: you leave at t = 0, at the very moment you arrived.

The universe is strongly deterministic. It always shows one consistent history: your lived past plus the rest of it, predicted by a solver. So your future, inverted self is there from the first moment, standing on `■`, and walks backward to meet you at the mirror. Deviate and the universe finds another consistent history; break your own past (say, by closing a door your earlier self already walked through) and it collapses in a paradox. The timeline under the board shows each stretch of your worldline as a swimlane, dashed where it's still only predicted.

Each level has a par, the fewest moves that finish it, shown as `moves/par`: `✓` when you finish within it, `✗` once you pass it. Live your future selves exactly as you first saw them, by memory and mirrored in time, and you earn *strong determinism*. After winning, your worldline becomes a tape of consciousness you can rewind and replay.

## Legend

| Glyph | Meaning |
|---|---|
| `●` / `○` | a self: filled is the one you control, hollow any other; red forward in time, blue inverted |
| `⋮` | glass: you can see through it, but not walk through |
| `╭───╮` | turnstile: red half forward, blue half inverted; each door closes while someone stands next to the mirror on its side |
| `⠀` → `⣿` | door, open → closed |
| `▶` / `■` | start / finish |
| `━` `╌` `┓┛` `╋` | timeline: a lived swimlane, a predicted one, an inversion joining two lanes, passing through a door |

Without color, inverted selves are drawn as diamonds (`◆` / `◇`).

## Controls

| Key | Action |
|---|---|
| Arrows or `WASD` | move |
| `U` | undo |
| `R` | restart |
| `?` | show or hide the legend |
| `J` / `K` / `L` | after winning: rewind / pause / play the tape (again for 2×, 4×) |
| `,` / `.` | after winning: step the tape back / forward |
| `Space` | after winning: play or pause the tape |
| `Q` / `Esc` | quit (terminal) |

On touch screens, swipe to move. After winning, tap to play or pause the tape and swipe left or right to step it.

## Build and run: terminal

Requires a Rust toolchain ([rustup](https://rustup.rs)).

```sh
git clone https://github.com/mmotorny/tenet-simulator.git
cd tenet-simulator
cargo run --release
```

Use a terminal with true-color support and a font that has box-drawing and Braille glyphs. Set `NO_COLOR=1` or pass `--no-color` for plain output.

Scripted runs print the final frame instead of opening the interactive view. Moves are `w`/`a`/`s`/`d`:

```sh
cargo run --release -- --moves ddwwdddddddddwwdddd    # the perfect run
cargo run --release -- path/to/level.txt              # play another level file
```

## Build and run: web

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130
./build-web.sh
python3 -m http.server 8080 --directory web/www
```

Then open http://localhost:8080.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The demo above is recorded by the game itself: the solver's perfect play, rendered frame by frame. After changing anything visible, regenerate it (a test fails while it is stale):

```sh
cargo run -q -- --demo-svg > docs/demo.svg
```

| Path | Contents |
|---|---|
| `core/` | Everything: levels, rules, the solver, the play session (undo, animations, the tape), a renderer-agnostic view and layout, and the painter shared by the web canvas and the SVG export. No I/O, no dependencies. |
| `core/levels/` | Level maps, one character per cell (legend in `core/src/level.rs`). |
| `cli/` | Terminal front end (crossterm): maps keys to session inputs and prints the screen. |
| `web/` | WebAssembly front end that draws the same screen onto a canvas, like a terminal emulator; the static page is in `www/`. |
| `docs/` | The README demo, generated with `--demo-svg`. |

Every push to `main` is tested, built and deployed to GitHub Pages by `.github/workflows/pages.yml`.

## License

[BSD 3-Clause](LICENSE).
