// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

// The game draws itself onto the canvas and runs its own animations (see web/src/lib.rs and the
// core's session); this script only maps keys and touches to inputs, keeps time, and redraws on
// resize and once fonts arrive.

import init, { WebGame } from "./pkg/tenet_web.js";

// Movement keys only ever move; the tape's transport keys (as in YouTube and video editors) only
// ever control the tape.
const KEYS = {
  ArrowUp: "north", w: "north",
  ArrowDown: "south", s: "south",
  ArrowRight: "east", d: "east",
  ArrowLeft: "west", a: "west",
  r: "restart", "?": "legend",
  j: "rewind", k: "pause", l: "play", " ": "toggle", ",": "back", ".": "forward",
};

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

  // Touch: the game decides what a gesture means (swipe the board to move, swipe the timeline
  // to scroll through the tape). Gestures elsewhere are the page's, so pulling down still
  // reloads it. Tapping the toolbar presses what's tapped.
  const touch = (kind) => (e) => {
    const t = e.changedTouches[0];
    const ours = game.touch(kind, t.clientX, t.clientY, e.timeStamp);
    if (ours && e.cancelable) e.preventDefault();
    draw();
    schedule();
  };
  canvas.addEventListener("touchstart", touch("down"), { passive: false });
  canvas.addEventListener("touchmove", touch("move"), { passive: false });
  canvas.addEventListener("touchend", touch("up"), { passive: false });
  // The mouse lights up and clicks the toolbar.
  const mouse = (kind) => (e) => {
    const over = game.mouse(kind, e.clientX, e.clientY);
    canvas.style.cursor = over ? "pointer" : "";
    draw();
    schedule();
  };
  canvas.addEventListener("mousemove", mouse("move"));
  canvas.addEventListener("mouseleave", mouse("leave"));
  canvas.addEventListener("mousedown", (e) => {
    if (e.button === 0) mouse("click")(e);
  });
  // A notch is about 100 pixels or 3 lines, whichever the wheel reports.
  const NOTCH = [100, 3, 1];
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    game.wheel(delta / NOTCH[e.deltaMode]);
    draw();
    schedule();
  }, { passive: false });

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
