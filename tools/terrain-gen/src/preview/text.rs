//! 5×7 bitmap font + text rendering for preview overlays. Hand-rolled to
//! avoid pulling a font crate for the 36-glyph base-36 + a few sign chars
//! that label settlements and region coordinates.

use image::{ImageBuffer, Rgb};

/// 5×7 bitmap glyphs indexed by base-36 digit (0..=9 then a..=z), top-to-
/// bottom rows, 5 LSB bits per row (bit 4 = leftmost pixel). Hand-rolled
/// to avoid pulling a font crate for a 36-glyph need.
const FONT_GLYPHS: [[u8; 7]; 36] = [
    [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E], // 0
    [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E], // 1
    [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F], // 2
    [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E], // 3
    [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02], // 4
    [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E], // 5
    [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E], // 6
    [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08], // 7
    [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E], // 8
    [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C], // 9
    [0x00, 0x00, 0x0E, 0x01, 0x0F, 0x11, 0x0F], // a
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x1E], // b
    [0x00, 0x00, 0x0E, 0x10, 0x10, 0x10, 0x0E], // c
    [0x01, 0x01, 0x0D, 0x13, 0x11, 0x11, 0x0F], // d
    [0x00, 0x00, 0x0E, 0x11, 0x1F, 0x10, 0x0E], // e
    [0x06, 0x09, 0x08, 0x1C, 0x08, 0x08, 0x08], // f
    [0x00, 0x00, 0x0F, 0x11, 0x0F, 0x01, 0x0E], // g
    [0x10, 0x10, 0x16, 0x19, 0x11, 0x11, 0x11], // h
    [0x04, 0x00, 0x0C, 0x04, 0x04, 0x04, 0x0E], // i
    [0x02, 0x00, 0x06, 0x02, 0x02, 0x12, 0x0C], // j
    [0x10, 0x10, 0x12, 0x14, 0x18, 0x14, 0x12], // k
    [0x0C, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E], // l
    [0x00, 0x00, 0x1A, 0x15, 0x15, 0x11, 0x11], // m
    [0x00, 0x00, 0x16, 0x19, 0x11, 0x11, 0x11], // n
    [0x00, 0x00, 0x0E, 0x11, 0x11, 0x11, 0x0E], // o
    [0x00, 0x00, 0x16, 0x19, 0x1E, 0x10, 0x10], // p
    [0x00, 0x00, 0x0D, 0x13, 0x0F, 0x01, 0x01], // q
    [0x00, 0x00, 0x16, 0x19, 0x10, 0x10, 0x10], // r
    [0x00, 0x00, 0x0F, 0x10, 0x0E, 0x01, 0x1E], // s
    [0x08, 0x08, 0x1C, 0x08, 0x08, 0x09, 0x06], // t
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x13, 0x0D], // u
    [0x00, 0x00, 0x11, 0x11, 0x11, 0x0A, 0x04], // v
    [0x00, 0x00, 0x11, 0x11, 0x15, 0x15, 0x0A], // w
    [0x00, 0x00, 0x11, 0x0A, 0x04, 0x0A, 0x11], // x
    [0x00, 0x00, 0x11, 0x11, 0x0F, 0x01, 0x0E], // y
    [0x00, 0x00, 0x1F, 0x02, 0x04, 0x08, 0x1F], // z
];

/// Glyph for "?" used for any char outside 0..=9 / a..=z (e.g. the "??"
/// overflow label from `settlement_label`).
const GLYPH_QUESTION: [u8; 7] = [0x0E, 0x11, 0x01, 0x02, 0x04, 0x00, 0x04];
const GLYPH_PLUS: [u8; 7] = [0x00, 0x04, 0x04, 0x1F, 0x04, 0x04, 0x00];
const GLYPH_MINUS: [u8; 7] = [0x00, 0x00, 0x00, 0x1F, 0x00, 0x00, 0x00];

fn font_glyph(c: char) -> [u8; 7] {
    match c {
        '+' => GLYPH_PLUS,
        '-' => GLYPH_MINUS,
        _ => c
            .to_digit(36)
            .map(|d| FONT_GLYPHS[d as usize])
            .unwrap_or(GLYPH_QUESTION),
    }
}

/// Stamp a 5×7 glyph at `(left, top)` scaled by `scale` (each lit bit becomes
/// a `scale×scale` block). X wraps, Y clamps — same conventions as `stamp_disk`.
fn draw_glyph(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    left: i32,
    top: i32,
    bitmap: &[u8; 7],
    color: Rgb<u8>,
    scale: i32,
) {
    for row in 0..7i32 {
        let bits = bitmap[row as usize];
        for col in 0..5i32 {
            if (bits >> (4 - col)) & 1 == 0 {
                continue;
            }
            for sy in 0..scale {
                for sx in 0..scale {
                    let py = top + row * scale + sy;
                    if py < 0 || py >= n as i32 {
                        continue;
                    }
                    let px = (left + col * scale + sx).rem_euclid(n as i32) as u32;
                    img.put_pixel(px, py as u32, color);
                }
            }
        }
    }
}

/// Pixel bbox `(left, top, right, bot)` of `text` rendered by
/// `draw_text_centered` at `scale`, centered on `(cx, cy)`. Right/bot are
/// exclusive. Empty strings collapse to a zero-width rect at the center.
fn text_bbox(text: &str, scale: i32, cx: i32, cy: i32) -> (i32, i32, i32, i32) {
    let char_w = 5 * scale;
    let char_h = 7 * scale;
    let gap = scale;
    let count = text.chars().count() as i32;
    let total_w = if count > 0 {
        count * char_w + (count - 1) * gap
    } else {
        0
    };
    let left = cx - total_w / 2;
    let top = cy - char_h / 2;
    (left, top, left + total_w, top + char_h)
}

/// Render `text` centered on `(cx, cy)` using the 5×7 bitmap font at `scale`.
/// Inter-character gap is 1 scaled pixel so dense IDs ("a0") stay readable.
pub(super) fn draw_text_centered(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    cx: i32,
    cy: i32,
    text: &str,
    color: Rgb<u8>,
    scale: i32,
) {
    let (mut left, top, _, _) = text_bbox(text, scale, cx, cy);
    let advance = 5 * scale + scale;
    for c in text.chars() {
        draw_glyph(img, n, left, top, &font_glyph(c), color, scale);
        left += advance;
    }
}

/// Draw `text` centered at `(cx, cy)` with a solid background pad behind it.
/// Used by the edge-region labels where text sits on arbitrary terrain colors
/// and needs guaranteed contrast (settlement labels rely on the yellow disk
/// behind them instead). `pad` is in scaled pixels around the glyph block.
pub(super) fn draw_text_with_bg(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    n: usize,
    cx: i32,
    cy: i32,
    text: &str,
    text_color: Rgb<u8>,
    bg_color: Rgb<u8>,
    scale: i32,
    pad: i32,
) {
    if text.is_empty() {
        return;
    }
    let (l, t, r, b) = text_bbox(text, scale, cx, cy);
    let x0 = (l - pad).max(0);
    let x1 = (r + pad).min(n as i32);
    let y0 = (t - pad).max(0);
    let y1 = (b + pad).min(n as i32);
    for py in y0..y1 {
        for px in x0..x1 {
            img.put_pixel(px as u32, py as u32, bg_color);
        }
    }
    draw_text_centered(img, n, cx, cy, text, text_color, scale);
}
