// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

import init, { WebGame } from "./pkg/tenet_web.js";

const COLOR_CLASSES = { r: "red", b: "blue", g: "glass", w: "wall", f: "floor", d: "door" };

const KEYS = {
  ArrowUp: "north", w: "north", k: "north",
  ArrowDown: "south", s: "south", j: "south",
  ArrowRight: "east", d: "east", l: "east",
  ArrowLeft: "west", a: "west", h: "west",
};

const map = document.getElementById("map");
const status = document.getElementById("status");

function span(text, cls) {
  const el = document.createElement("span");
  el.textContent = text;
  if (cls) el.className = cls;
  return el;
}

function draw(game) {
  const glyphRows = game.glyphs().split("\n");
  const colorRows = game.colors().split("\n");
  const labelRow = game.labelRow();
  map.replaceChildren();
  glyphRows.forEach((row, y) => {
    const glyphs = [...row];
    const colors = colorRows[y];
    // Group runs of the same color into one span.
    let start = 0;
    for (let x = 1; x <= glyphs.length; x++) {
      if (x === glyphs.length || colors[x] !== colors[start]) {
        map.append(span(glyphs.slice(start, x).join(""), COLOR_CLASSES[colors[start]]));
        start = x;
      }
    }
    if (y === labelRow) map.append(span(`    t = ${game.turn()} ▲`, "red"));
    map.append("\n");
  });
  status.textContent = game.exited()
    ? `You walked out of the exit door in ${game.turn()} moves. Press R to play again.`
    : "";
}

async function main() {
  await init();
  const game = new WebGame();

  const move = (dir) => {
    if (game.exited()) return;
    game.step(dir);
    draw(game);
  };

  document.addEventListener("keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
    if (key === "r") {
      game.reset();
      draw(game);
    } else if (KEYS[key]) {
      event.preventDefault();
      move(KEYS[key]);
    }
  });
  for (const button of document.querySelectorAll("#pad button")) {
    button.addEventListener("click", () => move(button.dataset.dir));
  }

  draw(game);
}

main().catch((error) => {
  status.textContent = `Failed to start: ${error}`;
  console.error(error);
});
