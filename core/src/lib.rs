// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Deterministic core of the Tenet simulator.
//!
//! Nothing in this crate does I/O, so it can be shared by the terminal and web front ends.

pub mod game;
pub mod level;
pub mod view;
