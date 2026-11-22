# Tenet Simulator

A simulator of strong determinism and time inversion, inspired by Nolan's *Tenet*. The goal is to build intuition for travelling through turnstiles and for solving time puzzles that seem unsolvable in the real world.

- 2D top-down, discrete time and space, pre-designed levels.
- Two targets: **terminal** (so the user and the agent can discuss diagrams in text) and **web** (for sharing).
- Status: one room with inversion, a timeline, par, undo, a winning celebration and a scrubbable tape. Consistency is lenient: past selves replay but aren't obstacles, and observations aren't checked yet (paradoxes are the next big design step).

## Rules (`core/src/game.rs`)

- Each move advances the protagonist's own time by one and global time by their arrow: +1 forward, −1 inverted.
- The turnstile is a red door, an even number of interior cells and a blue door in a line. Its mirror lies halfway. Crossing it moves the frontier of consciousness to the other side and reverses the arrow **without advancing global time**; the next move runs in the new direction.
- The two cells next to the mirror are **trapdoors**: each turnstile door is closed while anyone stands on the trapdoor on its side. Forward and inverted selves behave symmetrically about the mirror.
- A door can only be walked through while it is open, both now and on arrival: walking into a closed door is not a move, and doors never open because someone walks into them.
- The protagonist starts on `S` (a marker floor just outside the entry door) and finishes by stepping onto `F` (just outside the exit door). The empty space outside the room (` `) can't be entered, so it acts as invisible walls.
- The worldline is a list of segments, one per stretch between inversions. Earlier segments replay as other selves at the current global time. A self only exists during its segment, so it vanishes past `S`/`F` at other times (deliberately: not stuck on the marker).
- Level properties after the map: `entry-closes: 3`, `exit-closes: 3` (door open before that time) and `timeline: -2..12`.

## Session (`core/src/session.rs`)

- Wraps the game with undo (the move count follows the current path), the level's par (`core/src/solver.rs`, a breadth-first search run when the level loads), animations and the tape.
- Animations: door slides and the winning logo flip run "on twos" (83 ms per frame, 12 fps). Any input skips the current animation. There is no flash on winning.
- After winning: the logo flips a half turn (the slab squashes to edge-on and back), then the tape of consciousness rewinds by itself. The tape is the worldline in the protagonist's own order (lanes joined at inversions); the self controlled at each moment is drawn filled. It plays "on threes" (125 ms per frame at 1×; 2× and 4× on repeated presses).
- Front ends feed `Input`s, draw `Session::screen`, and call `advance` when `next_deadline` passes.

## Code layout

- Rust Cargo workspace.
- `core/` (`tenet-core`): everything — level parsing, rules, solver, session, a renderer-agnostic view (rows of glyphs with colors), the full-screen layout (`Screen::layout`/`grid`), the title logo, and the painter (`paint.rs`) that turns cells into drawing commands, plus the static and animated SVG exports. No I/O and no dependencies, so it compiles to WebAssembly.
- `core/levels/`: level maps, one character per cell (legend in `core/src/level.rs`), bundled via `core/src/levels.rs`. The first room is rotationally symmetric about the mirror, so its perfect play (par 19) is a palindrome ending at t = 0.
- `cli/` (`tenet-cli`, binary `tenet`): terminal front end built on crossterm.
- `web/` (`tenet-web`): draws the session's screen onto a `<canvas>` from the core's painter, snapped to device pixels (`src/`); the static page (`www/`) only maps keys and touches to inputs, keeps time and loads fonts. Text is JetBrains Mono from Fontsource on jsDelivr, with the GitHub monospace stack as fallback. `./build-web.sh` builds it; CI deploys it to https://mmotorny.github.io/tenet-simulator/ (see `.github/workflows/pages.yml`).

Commands:
- `cargo run` — play interactively.
- `cargo run -- --moves ddwwdd` — scripted run that prints the settled final frame (without the title). Use this to discuss runs as text.
- `cargo run -q -- --demo-svg > docs/demo.svg` — regenerate the README demo (the solver's perfect play with animations, the logo flip and the tape rewinding, as one looping animated SVG); a test fails while it is stale. GitHub can't align glyph rows in Markdown code blocks (font fallback), hence the SVG.
- `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` — keep both clean (CI enforces it).

## Controls

| Keys | Meaning |
|---|---|
| arrows, `wasd` | move (`hjkl` deliberately not: those are tape keys) |
| `u` / `r` | undo / restart |
| `j` `k` `l` | tape (after winning): rewind, pause, play; `j`/`l` again for 2×, 4× (as in YouTube and video editors) |
| `,` `.` | tape: step one frame back / forward |
| `space` | tape: play or pause, resuming the last direction |
| `q` `esc` | quit (terminal) |

No modes: movement keys only move, tape keys only control the tape. Touch: swipe to move; after winning, tap toggles the tape and horizontal swipes step it.

## Visual vocabulary

- **One glyph per cell**, colorless Unicode (no emoji).
- **Braille** is used selectively for graphics and animation within a cell, *not* as a dot-matrix rendering surface.
- **Colors:** red = forward in time, blue = inverted (film convention). Reserved for the arrow of time; never use them for anything else (e.g. not for a missed par). The environment stays neutral gray.
- **Selves:** filled `●` is the self you control, hollow `○` any other self (the frontier of consciousness); color gives the arrow. Without color (`NO_COLOR`, plain text), inverted selves become diamonds (`◆` / `◇`) via `view::monochrome`, so plain text keeps both meanings.

| Element | Glyph | Color |
|---|---|---|
| Selves | `●` you, `○` others; `◆`/`◇` for inverted without color | red forward / blue inverted |
| Room walls | `┏ ━ ┓ ┃ ┗ ┛` (heavy; `┣ ┫ ┳ ┻ ╋` junctions available) | gray |
| Floor | `·` | dark gray |
| Glass | `⋮` vertical, `⋯` horizontal (midline); occupies a cell | pale |
| Start / finish | `▶` / `■`, marker floors outside the entry / exit door | near-white |
| Doors (all doors) | `⠀` open → `⠉ ⠛ ⠿` → `⣿` closed (slide animation) | near-white (turnstile doors red/blue) |
| Turnstile frame | `╭ ─ ╮ ╰ ╯` (thin, rounded) | red half / blue half |
| Timeline | under the board: tick labels every 5 with the current time among them (colored by arrow; ticks too close to it are dropped), axis with a colored tick at now and `⣿` where doors close; one swimlane per segment (newest at the bottom; `━`, corners `┓┛` / `┏┗` connect lanes at inversions; events: `▶` start, `╋` passing a room door, `■` finish; `●`/`○` where each self is now) | the controlled lane red/blue, other lanes dim |
| Hint bar | led by the move counter `moves/par moves` (`✗` once past par, `✓` on finishing within it); keys bright, actions and `·` separators dim; arrows drawn as `←↑↓→` | near-white / dim gray |
| Title logo | 3 rows, letters roughly square like the poster, reading the same rotated 180° (`T E N Ǝ ⊥`): block stems (`▄ █ ▀`), heavy bars (`━ ╺ ╸`), N with heavy-line verticals (`╻ ┃ ╹`) and an evenly stepped quadrant-block diagonal; `core/src/title.rs` | black ink on a white slab (as in the film); slab edges are `▄`/`▀` half-block rows |

- Glass has **no solid junction glyphs** where it meets walls; the dotted rhythm reads as touching.
- Rejected: dashed glass (`╎ ┊ ¦`), glass as a thin wall between cells, glass inside the turnstile, turnstile embedded in the outer wall, a partial glass wall, Braille as a full rendering surface, thin line-art and 5-row block logos, light lines on the slab (too thin), walled alcoves around start/finish, start/finish inside the doorways (invisible when the door closes), a strikethrough for a missed par, a wall flash on winning.

## Turnstile

- A freestanding booth: red door, interior cells, blue door. No partition or midline is drawn; the mirror is invisible.
- Physically it would act as an invisible mirror: a surface that leads you into the past. To an outside observer, your forward self and your inverted self converge on it and annihilate (and emerge in pairs when leaving inversion). The film's booth hides that visual paradox.

## Working agreements

- **All logic lives in the core** (`tenet-core`): rules, rendering, animations, the tape, undo, par. The terminal and web apps are thin wrappers that map input to `session::Input` and draw `Session::screen`; never implement behaviour in a front end.
- **Iterate in the terminal first.** Prototype and refine changes (in the core) and show them through the terminal (the CLI, scripted `--moves` output, tmux captures). Don't build or check the web version, update the README demo or docs, commit, or push while iterating. When you think a change is done, ask the user whether to push; only then productize (web check, demo, docs, tests, commit, deploy).
- **Prototype before building** when the user asks to see something: print text mockups instead of implementing.
- When pasting rendered diagrams into chat, show the real glyphs. Never substitute placeholder marks (e.g. `_` for the blank open-door glyph `⠀`); describe invisible things in prose instead.
- The web page is just a terminal emulator: a canvas showing exactly the screen the CLI shows (same `Session::screen` and `Screen::layout`), one fixed-size cell per glyph. No extra web decoration — any decoration must be textual and shared with the terminal.
- Plain text stays readable: the CLI honors `NO_COLOR`, and `view::monochrome` swaps glyphs whose meaning relies on color.
- Licensed BSD-3-Clause (`LICENSE`). Every source file starts with SPDX headers:
  `// SPDX-License-Identifier: BSD-3-Clause` and `// SPDX-FileCopyrightText: 2026 Maksym Motorny`.
- Glyphs carry a foreground color, an optional background color (`Glyph::bg`) and a `bold` flag (web/SVG draw bold heavy strokes and diagonals at 4× light; SGR bold in the terminal).
- Like a terminal emulator, the painter (`core/src/paint.rs`) draws block elements (`view::block_quadrants`), box-drawing lines (`view::line_arms`), rounded corners and diagonals as geometry instead of font glyphs, so they join exactly across cells. The web canvas and the SVG exports use it; only other glyphs are drawn as text. The animated SVG writes each distinct shape once, grouped by the set of frames it appears in.
