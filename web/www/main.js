// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

// The game draws itself onto the canvas (see web/src/lib.rs); this script only wires up input,
// resizing and font loading.

import init, { WebGame } from "./pkg/tenet_web.js";

const KEYS = {
  ArrowUp: "north", w: "north", k: "north",
  ArrowDown: "south", s: "south", j: "south",
  ArrowRight: "east", d: "east", l: "east",
  ArrowLeft: "west", a: "west", h: "west",
};
const SWIPE_MIN_PX = 24;

const canvas = document.getElementById("screen");

async function main() {
  await init();
  const game = new WebGame();
  let pending = false;
  const draw = () => {
    if (pending) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      game.render(canvas);
    });
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

  // Redraw on resize and zoom (which changes devicePixelRatio), and once web fonts arrive; a
  // canvas doesn't reflow text by itself.
  window.addEventListener("resize", draw);
  document.fonts.addEventListener("loadingdone", draw);
  draw();
  await Promise.allSettled([
    document.fonts.load('13.6px "JetBrains Mono"'),
    document.fonts.load('bold 13.6px "JetBrains Mono"'),
  ]);
  draw();
}

main().catch((error) => {
  document.body.textContent = `failed to start: ${error}`;
  document.body.style.color = "#ccc";
  console.error(error);
});
