// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

// The page is a minimal terminal emulator: it shows the same screen as the terminal front end
// (title at the top, board in the middle, hint bar at the bottom), with every glyph in its own
// fixed-width cell so rows line up whatever fonts the browser picks.

import init, { WebGame } from "./pkg/tenet_web.js";

// CSS class per palette index; index 0 is the default color. Mirrors PALETTE in web/src/lib.rs.
const PALETTE_CLASSES = [null, "red", "blue", "glass", "wall", "floor", "door", "key", "hint", "ink", "slab"];
const KEYS = {
  ArrowUp: "north", w: "north", k: "north",
  ArrowDown: "south", s: "south", j: "south",
  ArrowRight: "east", d: "east", l: "east",
  ArrowLeft: "west", a: "west", h: "west",
};
const SWIPE_MIN_PX = 24;

// Block elements, as filled quadrants (upper-left, upper-right, lower-left, lower-right). Like a
// terminal emulator, the page paints these as solid rectangles instead of font glyphs, so blocks
// join seamlessly across cells. Mirrors view::block_quadrants in the core.
const QUADRANTS = {
  "█": "1111", "▀": "1100", "▄": "0011", "▌": "1010", "▐": "0101",
  "▘": "1000", "▝": "0100", "▖": "0010", "▗": "0001", "▚": "1001", "▞": "0110",
  "▛": "1110", "▜": "1101", "▙": "1011", "▟": "0111",
};

// Quadrant blocks: one conic gradient paints all four quadrants as a single full-cell layer
// (clockwise from the top: upper-right, lower-right, lower-left, upper-left). Separate half-size
// layers would get their sizes rounded to device pixels and leave hairline seams inside the cell.
// Full and half blocks are two-tone splits; a hard-stop linear gradient keeps their edges crisp.
const SPLITS = {
  "1111": "currentColor", "1100": "linear-gradient(currentColor 50%, transparent 50%)",
  "0011": "linear-gradient(transparent 50%, currentColor 50%)",
  "1010": "linear-gradient(to right, currentColor 50%, transparent 50%)",
  "0101": "linear-gradient(to right, transparent 50%, currentColor 50%)",
};

function paintBlock(cell, mask) {
  const [ul, ur, ll, lr] = [...mask].map((bit) => (bit === "1" ? "currentColor" : "transparent"));
  cell.classList.add("block");
  cell.style.setProperty("--quadrants", SPLITS[mask] ?? `conic-gradient(${ur} 0 25%, ${lr} 0 50%, ${ll} 0 75%, ${ul} 0)`);
}

// Box-drawing glyphs, painted like a terminal emulator does so lines meet exactly across cells:
// the directions each line leaves the cell in (left, right, up, down) and its weight.
// Each direction is followed by its weight: 1 light, 2 heavy. Mirrors view::line_arms in the core.
const LINES = {
  "─": "l1r1", "│": "u1d1", "┌": "r1d1", "┐": "l1d1", "└": "r1u1", "┘": "l1u1",
  "├": "u1d1r1", "┤": "u1d1l1", "┬": "l1r1d1", "┴": "l1r1u1", "┼": "l1r1u1d1", "╴": "l1", "╶": "r1",
  "━": "l2r2", "┃": "u2d2", "┏": "r2d2", "┓": "l2d2", "┗": "r2u2", "┛": "l2u2",
  // Heavy vertical, light horizontal.
  "┎": "d2r1", "┒": "d2l1", "┖": "u2r1", "┚": "u2l1", "┠": "u2d2r1", "┨": "u2d2l1", "┰": "l1r1d2", "┸": "l1r1u2",
};
// Curves and diagonals: SVG paths in a unit cell, stretched to the cell with a non-scaling stroke
// so their width matches the straight lines. Arcs are centred on the corner between their ends.
const PATHS = {
  "╭": "M1 .5A.5 .5 0 0 0 .5 1", "╮": "M0 .5A.5 .5 0 0 1 .5 1",
  "╰": "M.5 0A.5 .5 0 0 0 1 .5", "╯": "M.5 0A.5 .5 0 0 1 0 .5",
  "╲": "M0 0L1 1", "╱": "M0 1L1 0",
};
// Stroke widths by weight; heavy strokes of bold glyphs are drawn heavier still.
const STROKE = { 1: "var(--light)", 2: "var(--heavy)" };
const BOLD_STROKE = { 1: "var(--light)", 2: "var(--bold)" };

function bar(position, size) {
  return `linear-gradient(currentColor, currentColor) ${position} / ${size} no-repeat`;
}

// One arm from the cell edge to the centre, reaching across the perpendicular stroke's width.
function arm(dir, t, across) {
  const span = `calc(50% + ${across} / 2)`;
  const ends = { l: ["0 50%", `${span} ${t}`], r: ["100% 50%", `${span} ${t}`],
                 u: ["50% 0", `${t} ${span}`], d: ["50% 100%", `${t} ${span}`] };
  return bar(...ends[dir]);
}

function lineLayers(ch, bold) {
  const spec = LINES[ch];
  const stroke = bold ? BOLD_STROKE : STROKE;
  if (spec) {
    // Horizontal arms reach to the far edge of the vertical stroke, and vice versa, so mixed
    // weights still meet squarely in the middle.
    const parts = spec.match(/[lrud]\d/g).map((p) => [p[0], stroke[p[1]]]);
    const widest = (dirs) => parts.filter(([d]) => dirs.includes(d)).map(([, t]) => t);
    const reach = (ts) => (ts.length ? `max(${ts.join(", ")})` : "0px");
    const vertical = reach(widest("ud"));
    const horizontal = reach(widest("lr"));
    const weight = Object.fromEntries(parts);
    const layers = [];
    // A line passing straight through is one full-length bar: two half bars would each get their
    // size rounded to device pixels and could leave a gap where they meet.
    if (weight.l && weight.l === weight.r) layers.push(bar("0 50%", `100% ${weight.l}`));
    else for (const dir of ["l", "r"]) if (weight[dir]) layers.push(arm(dir, weight[dir], vertical));
    if (weight.u && weight.u === weight.d) layers.push(bar("50% 0", `${weight.u} 100%`));
    else for (const dir of ["u", "d"]) if (weight[dir]) layers.push(arm(dir, weight[dir], horizontal));
    return layers;
  }
  return null;
}

function paintLines(cell, layers) {
  cell.classList.add("block");
  cell.style.setProperty("--quadrants", layers.join(", "));
}

function paintPath(cell, d) {
  cell.classList.add("path");
  cell.innerHTML = `<svg viewBox="0 0 1 1" preserveAspectRatio="none"><path d="${d}"/></svg>`;
}

const term = document.getElementById("term");
const title = document.getElementById("title");
const board = document.getElementById("board");
const status = document.getElementById("status");

// Fills `el` with a block of cells: glyphs and their attributes in parallel arrays, indexed by
// y * width + x (see Cells in web/src/lib.rs).
function show(el, cells) {
  const { width, height, chars, fg, bg, bold } = cells;
  cells.free();
  const rows = [];
  for (let y = 0; y < height; y++) {
    const row = document.createElement("div");
    for (let i = y * width; i < (y + 1) * width; i++) {
      const ch = String.fromCodePoint(chars[i]);
      const cell = document.createElement("span");
      const mask = QUADRANTS[ch];
      const lines = lineLayers(ch, bold[i]);
      if (mask) paintBlock(cell, mask);
      else if (lines) paintLines(cell, lines);
      else if (PATHS[ch]) paintPath(cell, PATHS[ch]);
      else cell.textContent = ch;
      if (fg[i]) cell.classList.add(PALETTE_CLASSES[fg[i]]);
      if (bg[i]) cell.classList.add(`bg-${PALETTE_CLASSES[bg[i]]}`);
      if (bold[i]) cell.classList.add("bold");
      row.append(cell);
    }
    rows.push(row);
  }
  el.replaceChildren(...rows);
}

async function main() {
  await init();
  const game = new WebGame();
  const titleCells = game.title();
  title.style.setProperty("--width", titleCells.width);
  titleCells.free();
  board.style.setProperty("--width", game.boardWidth());
  const draw = () => {
    show(title, game.title());
    show(board, game.board());
    show(status, game.status());
  };
  const move = (dir) => {
    if (!game.exited()) {
      game.step(dir);
      draw();
    }
  };
  const restart = () => {
    game.reset();
    draw();
  };

  document.addEventListener("keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    if (key === "r") {
      restart();
    } else if (KEYS[key]) {
      event.preventDefault();
      move(KEYS[key]);
    }
  });

  // Touch: a swipe anywhere on the screen acts like a cursor key; a tap after leaving restarts.
  let touchStart = null;
  document.addEventListener("touchstart", (e) => {
    touchStart = { x: e.touches[0].clientX, y: e.touches[0].clientY };
  }, { passive: true });
  document.addEventListener("touchend", (e) => {
    if (!touchStart) return;
    const dx = e.changedTouches[0].clientX - touchStart.x;
    const dy = e.changedTouches[0].clientY - touchStart.y;
    touchStart = null;
    if (Math.max(Math.abs(dx), Math.abs(dy)) < SWIPE_MIN_PX) {
      if (game.exited()) restart();
    } else if (Math.abs(dx) > Math.abs(dy)) {
      move(dx > 0 ? "east" : "west");
    } else {
      move(dy > 0 ? "south" : "north");
    }
  });

  draw();
}

main().catch((error) => {
  term.textContent = `failed to start: ${error}`;
  console.error(error);
});
