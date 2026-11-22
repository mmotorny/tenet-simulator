# Tenet Simulator

A simulator of strong determinism and time inversion, inspired by Nolan's *Tenet*. The goal is to build intuition for travelling through turnstiles and for solving time puzzles that seem unsolvable in the real world.

- 2D top-down, discrete time and space, pre-designed levels.
- Two targets: **terminal** (so the user and the agent can discuss diagrams in text) and **web** (for sharing).
- Status: simple movement only (no time mechanics). The turnstile is a plain passage for now.

## Code layout

- Rust Cargo workspace.
- `core/` (`tenet-core`): level parsing, game rules, a renderer-agnostic view (rows of glyphs with colors), the full-screen layout (`Screen::layout`/`grid`), and the painter (`paint.rs`) that turns cells into drawing commands, plus the SVG export. No I/O and no dependencies, so it compiles to WebAssembly.
- `core/levels/`: level maps, one character per cell (legend in `core/src/level.rs`), bundled via `core/src/levels.rs`.
- `cli/` (`tenet-cli`, binary `tenet`): terminal front end built on crossterm.
- `web/` (`tenet-web`): draws the screen onto a `<canvas>` from the core's painter, snapped to device pixels (`src/`); the static page (`www/`) only handles input, resizing and font loading. Text is JetBrains Mono from Fontsource on jsDelivr, with the GitHub monospace stack as fallback. `./build-web.sh` builds it; CI deploys it to https://mmotorny.github.io/tenet-simulator/ (see `.github/workflows/pages.yml`).

Commands:
- `cargo run` — play interactively (arrows / wasd / hjkl, `r` to restart, `q` or `Esc` to quit). The screen is centered: title logo at the top, board in the middle, hint bar at the bottom.
- `cargo run -- --moves ddddww --trace` — scripted run that prints every frame (without the title). Use this to discuss runs as text.
- `cargo run -q -- --svg > docs/screenshot.svg` — regenerate the README screenshot; a test fails while it is stale. GitHub can't align glyph rows in Markdown code blocks (font fallback), hence the SVG.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` — keep both clean (CI enforces it).

## Visual vocabulary

- **One glyph per cell**, colorless Unicode (no emoji). Meaning must survive without color.
- **Braille** is used selectively for graphics and animation within a cell, *not* as a dot-matrix rendering surface.
- **Colors:** red = forward in time, blue = inverted (film convention). Reserved for things with an arrow of time; the environment stays neutral gray.

| Element | Glyph | Color |
|---|---|---|
| Forward self / inverted self | `●` / `○` | red / blue |
| Room walls | `┏ ━ ┓ ┃ ┗ ┛` (heavy) | gray |
| Floor | `·` | dark gray |
| Glass | `⋮` vertical, `⋯` horizontal (midline) | pale |
| Doors (all doors) | `⠀` open → `⠉ ⠛ ⠿` → `⣿` closed (slide animation) | near-white |
| Turnstile frame | `╭ ─ ╮ ╰ ╯` (thin, rounded) | red half / blue half |
| Hint bar | keys bright, actions and `·` separators dim; arrows drawn as `←↑↓→` | near-white / dim gray |
| Title logo | 3 rows, letters roughly square like the poster, reading the same rotated 180° (`T E N Ǝ ⊥`): block stems (`▄ █ ▀`), heavy bars (`━ ╺ ╸`), N with heavy-line verticals (`╻ ┃ ╹`) and an evenly stepped quadrant-block diagonal; `core/src/title.rs`. Rejected: thin line art, 5-row block letters, light lines on the slab (too thin) | black ink on a white slab (as in the film); slab edges are `▄`/`▀` half-block rows |

- Glass has **no solid junction glyphs** where it meets walls; the dotted rhythm reads as touching.
- Rejected: dashed glass (`╎ ┊ ¦`), glass inside the turnstile, turnstile embedded in the outer wall, a partial glass wall, Braille as a full rendering surface.

## Turnstile

- A freestanding booth with **two doors** (red side, blue side) and an **empty interior**: no partition, no midline.
- Physically it would act as an invisible mirror: a surface that leads you into the past. To an outside observer, your forward self and your inverted self converge on it and annihilate (and emerge in pairs when leaving inversion). The film's booth hides that visual paradox.
- Open question: where exactly inside the booth inversion happens, and how many steps walking through takes.

## Prototype level

```
┏━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ · · · · · · ⋮ · · · · · · ┃
┃ · · · · · · ⋮ · · · · · · ⠀    t = 0 ▲
┃ · · · · ╭───────╮ · · · · ┃
┃ · · · · ⠀       ⠀ · · · · ┃
┃ · · · · ╰───────╯ · · · · ┃
⠀ ● · · · · · ⋮ · · · · · · ┃
┃ · · · · · · ⋮ · · · · · · ┃
┗━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

- A full north–south glass wall splits the room (as in the freeport scene); the turnstile sits across it and is the only crossing.
- Enter through the west door. The east door is on a timer: open for all t < 9, closed afterwards.
- Solution: invert through the turnstile and walk out of the east door in the past.
- Timing needs recomputing once the booth-traversal rule is fixed.
- Open question: observation consistency (the forward self sees the inverted self through the glass). Recommendation: lenient during play, then replay the full consistent history after solving.

## Working agreements

- When pasting rendered diagrams into chat, show the real glyphs. Never substitute placeholder marks (e.g. `_` for the blank open-door glyph `⠀`); describe invisible things in prose instead.
- The web page is just a terminal emulator: a canvas showing exactly the screen the CLI shows (same `view::screen` and `Screen::layout`), one fixed-size cell per glyph. No extra web decoration — any decoration must be textual and shared with the terminal.
- Diagrams must stay readable as plain text: the CLI honors `NO_COLOR`, and glyphs (not just color) distinguish forward from inverted.
- Licensed BSD-3-Clause (`LICENSE`). Every source file starts with SPDX headers:
  `// SPDX-License-Identifier: BSD-3-Clause` and `// SPDX-FileCopyrightText: 2026 Maksym Motorny`.
- Glyphs carry a foreground color, an optional background color (`Glyph::bg`) and a `bold` flag (web/SVG draw bold heavy strokes and diagonals at 4× light; SGR bold in the terminal). The logo no longer uses `bold`; its weight comes from block elements.
- Like a terminal emulator, the painter (`core/src/paint.rs`) draws block elements (`view::block_quadrants`), box-drawing lines (`view::line_arms`), rounded corners and diagonals as geometry instead of font glyphs, so they join exactly across cells. Both the web canvas and the SVG export use it; only other glyphs are drawn as text.
