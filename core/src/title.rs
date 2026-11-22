// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! The title logo: TENET in thin, widely spaced line art.
//!
//! Like the film's logo, the word reads the same when rotated 180°: the right half "ET" is the
//! left half "TE" turned upside down, around a rotationally symmetric N. The forward half is red,
//! the inverted half blue, and the N, where the word turns around, is neutral.

use crate::view::{Color, Glyph};

const T: [&str; 5] = ["╶─┬─╴", "  │  ", "  │  ", "  │  ", "  │  "];
const E: [&str; 5] = ["┌───", "│   ", "├── ", "│   ", "└───"];
const N: [&str; 5] = ["│╲    │", "│ ╲   │", "│  ╲  │", "│   ╲ │", "│    ╲│"];
/// E rotated 180°.
const E_INV: [&str; 5] = ["───┐", "   │", " ──┤", "   │", "───┘"];
/// T rotated 180°.
const T_INV: [&str; 5] = ["  │  ", "  │  ", "  │  ", "  │  ", "╶─┴─╴"];

const LETTER_SPACING: usize = 2;

/// The logo as rows of glyphs, all the same width.
pub fn logo() -> Vec<Vec<Glyph>> {
    let letters: [(&[&str; 5], Color); 5] =
        [(&T, Color::Red), (&E, Color::Red), (&N, Color::Title), (&E_INV, Color::Blue), (&T_INV, Color::Blue)];
    (0..5)
        .map(|y| {
            let mut row = Vec::new();
            for (i, (letter, color)) in letters.iter().enumerate() {
                if i > 0 {
                    row.extend(std::iter::repeat_n(Glyph { ch: ' ', color: None }, LETTER_SPACING));
                }
                row.extend(letter[y].chars().map(|ch| Glyph { ch, color: (ch != ' ').then_some(*color) }));
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::to_plain_text;

    /// Rotating a line-art glyph by 180°.
    fn rotate(ch: char) -> char {
        match ch {
            '┌' => '┘',
            '┘' => '┌',
            '┐' => '└',
            '└' => '┐',
            '╮' => '╰',
            '╰' => '╮',
            '╭' => '╯',
            '╯' => '╭',
            '├' => '┤',
            '┤' => '├',
            '┬' => '┴',
            '┴' => '┬',
            '╴' => '╶',
            '╶' => '╴',
            other => other, // ─ │ ╲ and space are symmetric
        }
    }

    #[test]
    fn logo_reads_the_same_rotated_180_degrees() {
        let text = to_plain_text(&logo());
        let rotated: String = text.chars().rev().map(rotate).collect();
        assert_eq!(rotated, text);
    }

    #[test]
    fn logo_rows_have_equal_width() {
        let rows = logo();
        assert!(rows.iter().all(|r| r.len() == rows[0].len()));
    }
}
