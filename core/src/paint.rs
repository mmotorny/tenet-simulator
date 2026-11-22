// SPDX-License-Identifier: BSD-3-Clause
// SPDX-FileCopyrightText: 2026 Maksym Motorny

//! Painting cells into drawing commands, the way a terminal emulator draws its cell buffer.
//!
//! Text glyphs become [`Shape::Text`]; box-drawing lines, block elements, rounded corners and
//! diagonals are painted as geometry so they meet exactly across cells. Backends (the SVG export
//! below, the web canvas) only have to execute the shapes.

use crate::view::{block_quadrants, line_arms, Color, Glyph};

/// Default text color, for glyphs without one.
pub const DEFAULT_FG: (u8, u8, u8) = (200, 200, 200);

/// Cell geometry and stroke widths, in the backend's units.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub cell_w: f32,
    pub cell_h: f32,
    pub light: f32,
    pub heavy: f32,
    /// Heavy strokes of bold glyphs (and bold diagonals).
    pub bold: f32,
    /// How far painted bars and backgrounds overlap neighbouring cells, to hide anti-aliasing
    /// seams at fractional cell edges. Zero when snapping to whole pixels.
    pub bleed: f32,
    /// Round rectangle edges to whole units (device pixels), for crisp lines on a canvas.
    pub snap: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Rect { x: f32, y: f32, w: f32, h: f32, rgb: (u8, u8, u8) },
    /// A straight line with square caps (diagonals).
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, rgb: (u8, u8, u8) },
    /// An elliptical arc around (`cx`, `cy`), clockwise from `start` to `end` (radians, y down).
    Arc { cx: f32, cy: f32, rx: f32, ry: f32, start: f32, end: f32, width: f32, rgb: (u8, u8, u8) },
    /// A text glyph centred in the cell whose top-left corner is (`x`, `y`).
    Text { x: f32, y: f32, ch: char, rgb: (u8, u8, u8), bold: bool },
}

fn rgb(color: Option<Color>) -> (u8, u8, u8) {
    color.map_or(DEFAULT_FG, Color::rgb)
}

/// Paints a grid of glyphs with its top-left corner at (`ox`, `oy`). All backgrounds come first,
/// so no cell's background covers a neighbouring glyph.
pub fn paint(rows: &[Vec<Glyph>], m: &Metrics, ox: f32, oy: f32) -> Vec<Shape> {
    let s = |v: f32| if m.snap { v.round() } else { v };
    let mut shapes = Vec::new();

    for (y, row) in rows.iter().enumerate() {
        for (x, glyph) in row.iter().enumerate() {
            if let Some(bg) = glyph.bg {
                let (left, top) = (ox + x as f32 * m.cell_w, oy + y as f32 * m.cell_h);
                let (x0, y0) = (s(left - m.bleed), s(top - m.bleed));
                let (x1, y1) = (s(left + m.cell_w + m.bleed), s(top + m.cell_h + m.bleed));
                shapes.push(Shape::Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0, rgb: bg.rgb() });
            }
        }
    }

    for (y, row) in rows.iter().enumerate() {
        for (x, glyph) in row.iter().enumerate() {
            let (left, top) = (ox + x as f32 * m.cell_w, oy + y as f32 * m.cell_h);
            paint_glyph(&mut shapes, glyph, left, top, m);
        }
    }
    shapes
}

fn paint_glyph(shapes: &mut Vec<Shape>, glyph: &Glyph, left: f32, top: f32, m: &Metrics) {
    let s = |v: f32| if m.snap { v.round() } else { v };
    let (right, bottom) = (left + m.cell_w, top + m.cell_h);
    let (cx, cy) = (left + m.cell_w / 2.0, top + m.cell_h / 2.0);
    let color = rgb(glyph.color);
    // A rectangle between two corners, snapped edge by edge so neighbours share edges exactly.
    let mut rect = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let (x0, y0, x1, y1) = (s(x0), s(y0), s(x1), s(y1));
        shapes.push(Shape::Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0, rgb: color });
    };

    if let Some(quadrants) = block_quadrants(glyph.ch) {
        let xs = [left, cx, right];
        let ys = [top, cy, bottom];
        let b = m.bleed;
        for (i, filled) in quadrants.into_iter().enumerate() {
            if filled {
                let (qx, qy) = (i % 2, i / 2);
                rect(xs[qx] - b, ys[qy] - b, xs[qx + 1] + b, ys[qy + 1] + b);
            }
        }
        return;
    }

    if let Some([l, r, u, d]) = line_arms(glyph.ch) {
        let width = |w: u8| match w {
            2 if glyph.bold => m.bold,
            2 => m.heavy,
            _ => m.light,
        };
        // Arms reach across the perpendicular stroke so mixed weights meet squarely.
        let (across_h, across_v) = (width(u.max(d)), width(l.max(r)));
        let b = m.bleed;
        if l > 0 && l == r {
            let w = width(l);
            rect(left - b, cy - w / 2.0, right + b, cy + w / 2.0);
        } else {
            if l > 0 {
                let w = width(l);
                rect(left - b, cy - w / 2.0, cx + across_h / 2.0, cy + w / 2.0);
            }
            if r > 0 {
                let w = width(r);
                rect(cx - across_h / 2.0, cy - w / 2.0, right + b, cy + w / 2.0);
            }
        }
        if u > 0 && u == d {
            let w = width(u);
            rect(cx - w / 2.0, top - b, cx + w / 2.0, bottom + b);
        } else {
            if u > 0 {
                let w = width(u);
                rect(cx - w / 2.0, top - b, cx + w / 2.0, cy + across_v / 2.0);
            }
            if d > 0 {
                let w = width(d);
                rect(cx - w / 2.0, cy - across_v / 2.0, cx + w / 2.0, bottom + b);
            }
        }
        return;
    }

    use std::f32::consts::{FRAC_PI_2, PI};
    // Rounded corners: quarter ellipses centred on the cell corner between their two ends.
    let arc = match glyph.ch {
        '╭' => Some((right, bottom, PI, PI + FRAC_PI_2)),
        '╮' => Some((left, bottom, PI + FRAC_PI_2, 2.0 * PI)),
        '╰' => Some((right, top, FRAC_PI_2, PI)),
        '╯' => Some((left, top, 0.0, FRAC_PI_2)),
        _ => None,
    };
    if let Some((acx, acy, start, end)) = arc {
        let (rx, ry) = (m.cell_w / 2.0, m.cell_h / 2.0);
        shapes.push(Shape::Arc { cx: acx, cy: acy, rx, ry, start, end, width: m.light, rgb: color });
        return;
    }

    if glyph.ch == '╲' || glyph.ch == '╱' {
        let (y1, y2) = if glyph.ch == '╲' { (top, bottom) } else { (bottom, top) };
        let width = if glyph.bold { m.bold } else { m.light };
        shapes.push(Shape::Line { x1: left, y1, x2: right, y2, width, rgb: color });
        return;
    }

    if glyph.ch != ' ' && glyph.ch != '\u{2800}' {
        shapes.push(Shape::Text { x: left, y: top, ch: glyph.ch, rgb: color, bold: glyph.bold });
    }
}

const SVG_FONT: f32 = 14.0;

const SVG_METRICS: Metrics = Metrics {
    cell_w: SVG_FONT * 0.6,
    cell_h: SVG_FONT * 1.25,
    light: 1.2,
    heavy: 2.4,
    bold: 4.8,
    bleed: 0.25,
    snap: false,
};

fn svg_header(cols: usize, rows: usize) -> (String, f32) {
    let m = SVG_METRICS;
    let pad = 2.0 * m.cell_w;
    let width = cols as f32 * m.cell_w + 2.0 * pad;
    let height = rows as f32 * m.cell_h + 2.0 * pad;
    let header = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" \
         viewBox=\"0 0 {width} {height}\" font-family=\"ui-monospace, SFMono-Regular, 'SF Mono', Menlo, \
         Consolas, 'Liberation Mono', 'DejaVu Sans Mono', monospace\" font-size=\"{SVG_FONT}\" \
         text-anchor=\"middle\">\n<rect width=\"100%\" height=\"100%\" rx=\"6\" fill=\"#000\"/>\n"
    );
    (header, pad)
}

/// One shape as an SVG element.
fn shape_svg(shape: &Shape) -> String {
    let m = SVG_METRICS;
    let fill = |(r, g, b): (u8, u8, u8)| format!("rgb({r},{g},{b})");
    match *shape {
        Shape::Rect { x, y, w, h, rgb } => format!("<rect x=\"{x}\" y=\"{y}\" width=\"{w}\" height=\"{h}\" fill=\"{}\"/>", fill(rgb)),
        Shape::Line { x1, y1, x2, y2, width, rgb } => format!(
            "<line x1=\"{x1}\" y1=\"{y1}\" x2=\"{x2}\" y2=\"{y2}\" stroke=\"{}\" stroke-width=\"{width}\" stroke-linecap=\"square\"/>",
            fill(rgb)
        ),
        Shape::Arc { cx, cy, rx, ry, start, end, width, rgb } => {
            let (x1, y1) = (cx + rx * start.cos(), cy + ry * start.sin());
            let (x2, y2) = (cx + rx * end.cos(), cy + ry * end.sin());
            format!(
                "<path d=\"M{x1} {y1}A{rx} {ry} 0 0 1 {x2} {y2}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width}\"/>",
                fill(rgb)
            )
        }
        Shape::Text { x, y, ch, rgb, bold } => {
            let ch = match ch {
                '<' => "&lt;".to_string(),
                '>' => "&gt;".to_string(),
                '&' => "&amp;".to_string(),
                c => c.to_string(),
            };
            let weight = if bold { " font-weight=\"bold\"" } else { "" };
            format!("<text x=\"{}\" y=\"{}\" fill=\"{}\"{weight}>{ch}</text>", x + m.cell_w / 2.0, y + m.cell_h * 0.75, fill(rgb))
        }
    }
}

fn shapes_svg(rows: &[Vec<Glyph>], pad: f32) -> Vec<String> {
    paint(rows, &SVG_METRICS, pad, pad).iter().map(shape_svg).collect()
}

/// Renders rows as a standalone SVG image. Every glyph sits at its exact grid position, so rows
/// stay aligned whatever fonts the viewer substitutes (unlike text in a Markdown code block).
pub fn to_svg(rows: &[Vec<Glyph>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let (mut svg, pad) = svg_header(cols, rows.len());
    for shape in shapes_svg(rows, pad) {
        svg.push_str(&shape);
        svg.push('\n');
    }
    svg.push_str("</svg>\n");
    svg
}

/// Renders frames, each shown for its number of milliseconds, as one looping animated SVG.
///
/// Every distinct shape is written once. Shapes are grouped by the exact set of frames they appear
/// in, and each group's CSS keyframes show it during those frames; shapes in every frame need no
/// animation. Backgrounds keep their own groups, drawn before any glyph.
pub fn to_animated_svg(frames: &[(Vec<Vec<Glyph>>, u32)]) -> String {
    let cols = frames.iter().flat_map(|(rows, _)| rows.iter().map(Vec::len)).max().unwrap_or(0);
    let lines = frames.iter().map(|(rows, _)| rows.len()).max().unwrap_or(0);
    let (mut svg, pad) = svg_header(cols, lines);

    // For each layer (backgrounds, then glyphs): shapes in first-seen order, and the frames
    // each one appears in.
    let mut layers: [Vec<(String, Vec<bool>)>; 2] = [Vec::new(), Vec::new()];
    for (i, (rows, _)) in frames.iter().enumerate() {
        let backgrounds = rows.iter().flatten().filter(|g| g.bg.is_some()).count();
        for (k, shape) in shapes_svg(rows, pad).into_iter().enumerate() {
            let layer = &mut layers[usize::from(k >= backgrounds)];
            match layer.iter_mut().find(|(s, _)| *s == shape) {
                Some((_, seen)) => seen[i] = true,
                None => {
                    let mut seen = vec![false; frames.len()];
                    seen[i] = true;
                    layer.push((shape, seen));
                }
            }
        }
    }

    let total: u32 = frames.iter().map(|(_, ms)| ms).sum::<u32>().max(1);
    let starts: Vec<u32> = frames.iter().scan(0, |t, (_, ms)| Some(std::mem::replace(t, *t + ms))).collect();
    let pct = |ms: u32| format!("{:.4}%", f64::from(ms) * 100.0 / f64::from(total));
    let mut patterns: Vec<Vec<bool>> = Vec::new();
    let mut body = String::new();
    for layer in &layers {
        // Group this layer's shapes by visibility pattern, keeping the order patterns first appear in.
        let mut groups: Vec<(Vec<bool>, Vec<&String>)> = Vec::new();
        for (shape, seen) in layer {
            match groups.iter_mut().find(|(p, _)| p == seen) {
                Some((_, shapes)) => shapes.push(shape),
                None => groups.push((seen.clone(), vec![shape])),
            }
        }
        for (pattern, shapes) in groups {
            let always = pattern.iter().all(|&v| v);
            if !always {
                let id = patterns.iter().position(|p| *p == pattern).unwrap_or_else(|| {
                    patterns.push(pattern.clone());
                    patterns.len() - 1
                });
                body.push_str(&format!("<g class=\"f\" style=\"animation-name:k{id}\">\n"));
            }
            for shape in shapes {
                body.push_str(shape);
                body.push('\n');
            }
            if !always {
                body.push_str("</g>\n");
            }
        }
    }

    let mut style = format!(".f{{opacity:0;animation:{total}ms step-end infinite}}\n");
    for (id, pattern) in patterns.iter().enumerate() {
        // A keyframe wherever visibility changes; step-end holds each value until the next.
        let mut keys = format!("0%{{opacity:{}}}", u8::from(pattern[0]));
        for i in 1..pattern.len() {
            if pattern[i] != pattern[i - 1] {
                keys.push_str(&format!("{}{{opacity:{}}}", pct(starts[i]), u8::from(pattern[i])));
            }
        }
        style.push_str(&format!("@keyframes k{id}{{{keys}}}\n"));
    }
    svg.push_str(&format!("<style>\n{style}</style>\n"));
    svg.push_str(&body);
    svg.push_str("</svg>\n");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str, color: Color) -> Vec<Glyph> {
        s.chars().map(|ch| Glyph::new(ch, color)).collect()
    }

    const UNIT: Metrics =
        Metrics { cell_w: 10.0, cell_h: 20.0, light: 1.0, heavy: 2.0, bold: 4.0, bleed: 0.0, snap: true };

    #[test]
    fn lines_blocks_arcs_and_diagonals_are_geometry_text_is_text() {
        let shapes = paint(&[text("━┏a ⠀█╭╲", Color::Wall)], &UNIT, 0.0, 0.0);
        let count = |f: fn(&Shape) -> bool| shapes.iter().filter(|s| f(s)).count();
        assert_eq!(count(|s| matches!(s, Shape::Text { .. })), 1); // only "a"; blanks are skipped
        assert_eq!(count(|s| matches!(s, Shape::Rect { .. })), 1 + 2 + 4); // ━ bar, ┏ two arms, █ quadrants
        assert_eq!(count(|s| matches!(s, Shape::Arc { .. })), 1);
        assert_eq!(count(|s| matches!(s, Shape::Line { .. })), 1);
    }

    #[test]
    fn straight_lines_span_the_cell_and_meet_their_neighbours() {
        let shapes = paint(&[text("──", Color::Wall)], &UNIT, 0.0, 0.0);
        assert_eq!(shapes[0], Shape::Rect { x: 0.0, y: 10.0, w: 10.0, h: 1.0, rgb: Color::Wall.rgb() });
        assert_eq!(shapes[1], Shape::Rect { x: 10.0, y: 10.0, w: 10.0, h: 1.0, rgb: Color::Wall.rgb() });
    }

    #[test]
    fn bold_heavy_strokes_use_the_bold_width() {
        let shapes = paint(&[vec![Glyph::new('┃', Color::Ink).bold()]], &UNIT, 0.0, 0.0);
        assert!(matches!(shapes[0], Shape::Rect { w, .. } if w == 4.0));
        let shapes = paint(&[vec![Glyph::new('┃', Color::Ink)]], &UNIT, 0.0, 0.0);
        assert!(matches!(shapes[0], Shape::Rect { w, .. } if w == 2.0));
    }

    #[test]
    fn backgrounds_come_before_any_glyph() {
        let rows = vec![vec![Glyph::on('─', Some(Color::Ink), Color::Slab), Glyph::on(' ', None, Color::Slab)]];
        let shapes = paint(&rows, &UNIT, 0.0, 0.0);
        let first_glyph = shapes.iter().position(|s| s != &shapes[0] && !matches!(s, Shape::Rect { rgb, .. } if *rgb == Color::Slab.rgb())).unwrap();
        assert_eq!(first_glyph, 2); // two background rects, then the line
    }

    #[test]
    fn animated_svg_draws_common_shapes_once_and_times_every_frame() {
        let a = vec![text("┃a", Color::Wall)];
        let b = vec![text("┃b", Color::Wall)];
        let svg = to_animated_svg(&[(a, 100), (b, 300)]);
        assert_eq!(svg.matches("<g class=\"f\"").count(), 2);
        assert_eq!(svg.matches("<rect x=").count(), 1); // the shared ┃ is drawn once, outside any group
        assert!(svg.contains("animation:400ms step-end infinite"));
        assert!(svg.contains("@keyframes k0{0%{opacity:1}25.0000%{opacity:0}}")); // "a": the first 100 ms
        assert!(svg.contains("@keyframes k1{0%{opacity:0}25.0000%{opacity:1}}")); // "b": the rest
    }

    #[test]
    fn svg_escapes_text_and_paints_backgrounds_under_spaces() {
        let svg = to_svg(&[text("a<", Color::Wall), vec![Glyph::on(' ', None, Color::Slab)]]);
        assert!(svg.contains("&lt;"));
        assert_eq!(svg.matches("<text ").count(), 2);
        assert_eq!(svg.matches("<rect x=").count(), 1);
    }
}
