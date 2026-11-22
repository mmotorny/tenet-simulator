#!/usr/bin/env bash
# SPDX-License-Identifier: BSD-3-Clause
# SPDX-FileCopyrightText: 2026 Maksym Motorny

# Builds the web front end into web/www (serve that directory).
# Needs: rustup target add wasm32-unknown-unknown; cargo install wasm-bindgen-cli --version 0.2.130
set -euo pipefail
cd "$(dirname "$0")"

cargo build -p tenet-web --target wasm32-unknown-unknown --release
wasm-bindgen --target web --no-typescript --out-dir web/www/pkg \
  target/wasm32-unknown-unknown/release/tenet_web.wasm
ls -lh web/www/pkg/tenet_web_bg.wasm
