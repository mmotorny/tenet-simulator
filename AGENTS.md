# Agent instructions

## Keeping this file lean

- Only add instructions that change agent behavior and generalize to future work. If the code, the README or the git history already says it, or an agent would do it anyway, leave it out.
- Don't describe the code, rules or visuals here; they live in the code. Record decisions only when an agent would otherwise get them wrong (for example, options the user already rejected).

## Workflow

- **All logic lives in the core** (`tenet-core`). The terminal and web apps only map input to `session::Input` and draw `Session::screen`; never implement behaviour in a front end, even while iterating.
- **Iterate fast, productize on request.** While iterating, show changes through the terminal (`cargo run -- --moves …` prints the settled final frame as text; tmux captures for live behaviour). Don't check the web version, regenerate the demo, update docs, fix stale tests, commit or push. When a change seems done, ask whether to push; then do all of those.
- **Show before building** when the user asks to see something: print text mockups in chat, don't implement.
- When pasting renders into chat, show the real glyphs; never substitute placeholders (e.g. `_` for the blank `⠀`). Describe invisible things in prose.
- Productizing includes `cargo run -q -- --demo-svg > docs/demo.svg` (a test fails while the README demo is stale) and `cargo clippy -p tenet-web --target wasm32-unknown-unknown -- -D warnings`, which the native clippy run doesn't cover.

## Verifying visuals

- Look at results yourself before reporting: tmux for the terminal, headless Chrome screenshots for the web and SVGs.
- Use a fresh `--user-data-dir` for every headless Chrome screenshot; a cached stylesheet once hid a regression.
- Headless Chrome's virtual time doesn't advance CSS animations inside `<img>`; to check an animated SVG, inline it and seek with `document.getAnimations()`.
- tmux captures can catch a frame mid-redraw, and seeking an inlined animated SVG can show two frames at once; confirm suspicious frames deterministically (in code, or from the SVG's keyframes).

## Design constraints

- Paradoxes: don't add special-case detection, verification or resolution logic around the prediction. The user decides how each paradox should resolve as they encounter it. Standing on a trapdoor is never illegal (it may call for another self).
- The solver's search is bounded by a fixed move budget, honestly reported as "within N moves". Don't add search bounds (time windows and the like) without a proof that they're safe.
- The web page is a terminal emulator: exactly the terminal's screen, one cell per glyph. No web-only decoration; any decoration is textual and shared.
- One colorless Unicode glyph per cell (no emoji). Braille only for in-cell graphics, never as a dot-matrix surface.
- Red and blue mean the arrow of time (forward, inverted) and nothing else. The one sanctioned exception: the strong determinism reveal animates its letters red from the start and blue from the end, meeting in the middle.
- Plain text must keep every meaning: when color carries meaning, add a glyph swap in `view::monochrome`.
- Rejected, don't re-propose: dashed glass (`╎ ┊ ¦`); glass as a thin wall between cells; glass inside the turnstile; a turnstile in the outer wall; a partial glass wall; thin line-art or 5-row block logos; light lines on the logo slab; walls around start/finish; start/finish inside doorways; `hjkl` for movement; strikethrough for a missed par; a wall flash on winning; a double logo turn for the bonus; selves stuck on start/finish outside their worldline; `u` undo (the tape replaces it); rewinding to the history as predicted back then (the committed tape stays); a restart that keeps the tape; tapping the timeline; dragging with the mouse (it only wheels and clicks the hint bar); `‹ ›` on the time axis.

## Repository conventions

- Every source file starts with `// SPDX-License-Identifier: BSD-3-Clause` and `// SPDX-FileCopyrightText: 2026 Maksym Motorny` (BSD-3-Clause).

## Environment gotchas

- The shell is zsh on macOS: `$var` doesn't word-split (pass arguments explicitly), unmatched globs abort the command, and `sed -i` needs `''`. Prefer small Python scripts for multi-line edits.
