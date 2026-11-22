// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! The title logo: TENET in thin, widely spaced line art.
//!
//! Like the film's logo, the word reads the same when rotated 180°: the right half "ET" is the
//! left half "TE" turned upside down, around a rotationally symmetric N. As in the film, the
//! letters are black; they sit on a white slab.

use crate::view::{Color, Glyph};

// As in the film's logo, vertical strokes are heavier than horizontal ones: mixed-weight
// box-drawing glyphs (heavy vertical, light horizontal). Diagonals are bold too, but Unicode has
// no heavy diagonal, so they carry the bold attribute instead.
const T: [&str; 5] = ["╶─┰─╴", "  ┃  ", "  ┃  ", "  ┃  ", "  ┃  "];
const E: [&str; 5] = ["┎───", "┃   ", "┠── ", "┃   ", "┖───"];
const N: [&str; 5] = ["┃╲    ┃", "┃ ╲   ┃", "┃  ╲  ┃", "┃   ╲ ┃", "┃    ╲┃"];
/// E rotated 180°.
const E_INV: [&str; 5] = ["───┒", "   ┃", " ──┨", "   ┃", "───┚"];
/// T rotated 180°.
const T_INV: [&str; 5] = ["  ┃  ", "  ┃  ", "  ┃  ", "  ┃  ", "╶─┸─╴"];

const LETTER_SPACING: usize = 2;
/// Slab padding left and right of the letters, in cells. Above and below, the slab extends by a
/// half-block row, which is about as tall as two cells are wide.
const SLAB_PADDING: usize = 2;

/// The bare letters as text rows, all the same width.
fn letters() -> Vec<String> {
    let glyphs = [&T, &E, &N, &E_INV, &T_INV];
    (0..T.len()).map(|y| glyphs.iter().map(|g| g[y]).collect::<Vec<_>>().join(&" ".repeat(LETTER_SPACING))).collect()
}

/// The logo: black letters on a white slab, as rows of glyphs that are all the same width.
pub fn logo() -> Vec<Vec<Glyph>> {
    let rows = letters();
    let width = rows[0].chars().count() + 2 * SLAB_PADDING;
    let edge = |ch| vec![Glyph::new(ch, Color::Slab); width];
    let pad = || std::iter::repeat_n(Glyph::on(' ', None, Color::Slab), SLAB_PADDING);

    let mut logo = vec![edge('▄')];
    for row in rows {
        let mut line: Vec<Glyph> = pad().collect();
        line.extend(row.chars().map(|ch| {
            let glyph = Glyph::on(ch, (ch != ' ').then_some(Color::Ink), Color::Slab);
            if ch == '╲' { glyph.bold() } else { glyph }
        }));
        line.extend(pad());
        logo.push(line);
    }
    logo.push(edge('▀'));
    logo
}

#[cfg(test)]
mod tests {
    use super::*;

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
            '┎' => '┚',
            '┚' => '┎',
            '┒' => '┖',
            '┖' => '┒',
            '┠' => '┨',
            '┨' => '┠',
            '┰' => '┸',
            '┸' => '┰',
            other => other, // ─ │ ┃ ╲ and space are symmetric
        }
    }

    #[test]
    fn logo_reads_the_same_rotated_180_degrees() {
        let text = letters().join("\n");
        let rotated: String = text.chars().rev().map(rotate).collect();
        assert_eq!(rotated, text);
    }

    #[test]
    fn logo_rows_have_equal_width() {
        let rows = logo();
        assert!(rows.iter().all(|r| r.len() == rows[0].len()));
    }

    #[test]
    fn letters_are_black_on_a_white_slab() {
        let rows = logo();
        assert_eq!((rows.len(), rows[0].len()), (7, 37));
        assert!(rows[0].iter().all(|g| g.ch == '▄' && g.color == Some(Color::Slab) && g.bg.is_none()));
        assert!(rows[6].iter().all(|g| g.ch == '▀' && g.color == Some(Color::Slab) && g.bg.is_none()));
        assert!(rows[1..6].iter().flatten().all(|g| g.bg == Some(Color::Slab)));
        assert!(rows[1..6].iter().flatten().filter(|g| g.ch != ' ').all(|g| g.color == Some(Color::Ink)));
        assert!(rows[1..6].iter().flatten().all(|g| g.bold == (g.ch == '╲'))); // only diagonals are bold
    }
}
