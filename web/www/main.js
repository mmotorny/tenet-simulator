// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

// The game draws itself onto the canvas and runs its own animations (see web/src/lib.rs and the
// core's session); this script only maps keys and touches to inputs, keeps time, and redraws on
// resize and once fonts arrive.

import init, { WebGame } from "./pkg/tenet_web.js";

// Movement keys only ever move; the tape's transport keys (as in YouTube and video editors) only
// ever control the tape after winning.
const KEYS = {
  ArrowUp: "north", w: "north",
  ArrowDown: "south", s: "south",
  ArrowRight: "east", d: "east",
  ArrowLeft: "west", a: "west",
  u: "undo", r: "restart", "?": "legend",
  j: "rewind", k: "pause", l: "play", " ": "toggle", ",": "back", ".": "forward",
};
const SWIPE_MIN_PX = 24;

const canvas = document.getElementById("screen");

async function main() {
  await init();
  const game = new WebGame();

  let frame = 0;
  const draw = () => {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      game.render(canvas);
    });
  };
  // Animations and the playing tape move on by themselves at the session's deadlines.
  let timer = 0;
  const schedule = () => {
    clearTimeout(timer);
    const ms = game.nextDeadline();
    if (ms >= 0) {
      timer = setTimeout(() => {
        game.advance();
        draw();
        schedule();
      }, ms);
    }
  };
  const input = (name) => {
    game.input(name);
    draw();
    schedule();
  };

  document.addEventListener("keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    if (KEYS[key]) {
      event.preventDefault();
      input(KEYS[key]);
    }
  });

  // Touch: a swipe anywhere acts like a cursor key. After winning there is nothing to move, so a
  // tap plays or pauses the tape and a horizontal swipe steps it.
  let touchStart = null;
  document.addEventListener("touchstart", (e) => {
    touchStart = { x: e.touches[0].clientX, y: e.touches[0].clientY };
  }, { passive: true });
  document.addEventListener("touchend", (e) => {
    if (!touchStart) return;
    const dx = e.changedTouches[0].clientX - touchStart.x;
    const dy = e.changedTouches[0].clientY - touchStart.y;
    touchStart = null;
    const tap = Math.max(Math.abs(dx), Math.abs(dy)) < SWIPE_MIN_PX;
    if (game.exited()) {
      if (tap) input("toggle");
      else if (Math.abs(dx) > Math.abs(dy)) input(dx > 0 ? "forward" : "back");
    } else if (!tap) {
      if (Math.abs(dx) > Math.abs(dy)) input(dx > 0 ? "east" : "west");
      else input(dy > 0 ? "south" : "north");
    }
  });

  // A canvas doesn't reflow by itself: redraw on resize and zoom, and once web fonts arrive.
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
