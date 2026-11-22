// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

// The page is a minimal terminal emulator: it shows the same screen as the terminal front end,
// with every glyph in its own fixed-width cell so rows line up whatever fonts the browser picks.

import init, { WebGame } from "./pkg/tenet_web.js";

const HINT = "arrows / wasd / hjkl / swipe move · r restart";
const EXIT_HINT = " · r restart";
const COLOR_CLASSES = { r: "red", b: "blue", g: "glass", w: "wall", f: "floor", d: "door" };
const KEYS = {
  ArrowUp: "north", w: "north", k: "north",
  ArrowDown: "south", s: "south", j: "south",
  ArrowRight: "east", d: "east", l: "east",
  ArrowLeft: "west", a: "west", h: "west",
};
const SWIPE_MIN_PX = 24;

const term = document.getElementById("term");

function show(screen) {
  term.replaceChildren(
    ...screen.split("\n").map((encoded) => {
      const row = document.createElement("div");
      const chars = [...encoded];
      for (let i = 0; i + 1 < chars.length; i += 2) {
        const cell = document.createElement("span");
        cell.textContent = chars[i];
        const color = COLOR_CLASSES[chars[i + 1]];
        if (color) cell.className = color;
        row.append(cell);
      }
      return row;
    }),
  );
}

async function main() {
  await init();
  const game = new WebGame();
  const draw = () => show(game.screen(HINT, EXIT_HINT));
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

  // Touch: swipe to move; tap after leaving to restart.
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
