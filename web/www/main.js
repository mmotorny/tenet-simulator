// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

// The page is a minimal terminal emulator: it shows the same screen as the terminal front end
// (title at the top, board in the middle, hint bar at the bottom), with every glyph in its own
// fixed-width cell so rows line up whatever fonts the browser picks.

import init, { WebGame } from "./pkg/tenet_web.js";

const COLOR_CLASSES = {
  r: "red", b: "blue", g: "glass", w: "wall", f: "floor", d: "door", k: "key", h: "hint", t: "title",
};
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

// One conic gradient paints all four quadrants as a single full-cell layer (clockwise from the
// top: upper-right, lower-right, lower-left, upper-left). Separate half-size layers would get
// their sizes rounded to device pixels and leave hairline seams inside the cell.
function paintBlock(cell, mask) {
  const [ul, ur, ll, lr] = [...mask].map((bit) => (bit === "1" ? "currentColor" : "transparent"));
  cell.classList.add("block");
  cell.style.setProperty("--quadrants", `conic-gradient(${ur} 0 25%, ${lr} 0 50%, ${ll} 0 75%, ${ul} 0)`);
}

const term = document.getElementById("term");
const title = document.getElementById("title");
const board = document.getElementById("board");
const status = document.getElementById("status");

// Fills `el` with encoded rows: each glyph followed by its color code (see web/src/lib.rs).
function show(el, encodedRows) {
  el.replaceChildren(
    ...encodedRows.split("\n").map((encoded) => {
      const row = document.createElement("div");
      const chars = [...encoded];
      for (let i = 0; i + 1 < chars.length; i += 2) {
        const cell = document.createElement("span");
        const mask = QUADRANTS[chars[i]];
        if (mask) paintBlock(cell, mask);
        else cell.textContent = chars[i];
        const color = COLOR_CLASSES[chars[i + 1]];
        if (color) cell.classList.add(color);
        row.append(cell);
      }
      return row;
    }),
  );
}

async function main() {
  await init();
  const game = new WebGame();
  title.style.setProperty("--width", [...game.title().split("\n")[0]].length / 2);
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
