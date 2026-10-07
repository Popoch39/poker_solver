//! Reads the client's text: a 5×7 bitmap font drawn at scale 2, never
//! anti-aliased, one colour on a flat background.
//!
//! The glyph shapes are cut out of a status line the client renders; only
//! the grid they sit on is assumed here.

use std::collections::HashMap;

use nitro_local_client::{Frame, LAYOUT, Rect, TEXT_HEIGHT};

/// Pixels from one glyph to the next: 5 font columns and 1 of spacing, at
/// scale 2.
const ADVANCE: u32 = 12;
/// Blank pixels after the last glyph of a string.
const SPACING: u32 = 2;
/// Every character the client draws, but the space.
pub(crate) const CHARSET: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-/.:()";
/// A colour channel further than this from the background is ink.
const INK_THRESHOLD: u8 = 40;
/// Pixels a glyph may differ from its template and still be read.
const MAX_GLYPH_ERRORS: u32 = 3;

/// One glyph cell: a row of ink bits per pixel row, bit 0 leftmost.
type Cell = [u16; TEXT_HEIGHT as usize];

pub(crate) struct Font {
    glyphs: HashMap<Cell, char>,
}

impl Font {
    /// The glyphs of `CHARSET`, drawn left-aligned on the status line of
    /// `frame`.
    pub(crate) fn from_status_line(frame: &Frame) -> Self {
        let rect = LAYOUT.status;
        let background = background(frame, rect);
        let mut glyphs: HashMap<Cell, char> = CHARSET
            .chars()
            .enumerate()
            .map(|(i, c)| {
                let cell = cell(frame, rect.x + i as u32 * ADVANCE, rect.y, background);
                (cell, c)
            })
            .collect();
        assert_eq!(glyphs.len(), CHARSET.len(), "every glyph is distinct");
        glyphs.insert([0; TEXT_HEIGHT as usize], ' ');
        Self { glyphs }
    }

    /// The text drawn from the left edge of `rect`, without trailing
    /// spaces; `None` when a glyph is not recognised.
    pub(crate) fn left(&self, frame: &Frame, rect: Rect) -> Option<String> {
        let background = background(frame, rect);
        let n = rect.width.div_ceil(ADVANCE);
        let text = self.decode(frame, rect, rect.x, n, background)?;
        Some(text.trim_end().to_owned())
    }

    /// The text drawn centred in `rect` (empty when there is none); `None`
    /// when a glyph is not recognised.
    pub(crate) fn centered(&self, frame: &Frame, rect: Rect) -> Option<String> {
        let background = background(frame, rect);
        let Some((first, last)) = ink_columns(frame, rect, background) else {
            return Some(String::new());
        };
        // The client centres a string of n glyphs on its exact width, so
        // the length alone fixes where every glyph starts.
        (1..=(rect.width + SPACING) / ADVANCE).find_map(|n| {
            let width = n * ADVANCE - SPACING;
            let start = rect.x + (rect.width - width) / 2;
            if first < start || last >= start + width {
                return None;
            }
            let text = self.decode(frame, rect, start, n, background)?;
            (!text.starts_with(' ') && !text.ends_with(' ')).then_some(text)
        })
    }

    fn decode(
        &self,
        frame: &Frame,
        rect: Rect,
        x: u32,
        n: u32,
        background: [u8; 4],
    ) -> Option<String> {
        (0..n)
            .map(|i| self.glyph(&cell(frame, x + i * ADVANCE, rect.y, background)))
            .collect()
    }

    fn glyph(&self, cell: &Cell) -> Option<char> {
        if let Some(&c) = self.glyphs.get(cell) {
            return Some(c);
        }
        self.glyphs
            .iter()
            .map(|(glyph, &c)| (distance(glyph, cell), c))
            .filter(|&(errors, _)| errors <= MAX_GLYPH_ERRORS)
            .min_by_key(|&(errors, _)| errors)
            .map(|(_, c)| c)
    }
}

fn distance(a: &Cell, b: &Cell) -> u32 {
    a.iter().zip(b).map(|(a, b)| (a ^ b).count_ones()).sum()
}

/// The most common colour of `rect`: what its text is drawn on.
fn background(frame: &Frame, rect: Rect) -> [u8; 4] {
    let mut counts: HashMap<[u8; 4], u32> = HashMap::new();
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            *counts.entry(frame.pixel(x, y)).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(color, count)| (count, color))
        .map(|(color, _)| color)
        .expect("a non-empty rectangle")
}

fn is_ink(frame: &Frame, x: u32, y: u32, background: [u8; 4]) -> bool {
    if x >= frame.width() || y >= frame.height() {
        return false;
    }
    let pixel = frame.pixel(x, y);
    pixel
        .iter()
        .zip(background)
        .take(3)
        .any(|(&p, b)| p.abs_diff(b) > INK_THRESHOLD)
}

fn cell(frame: &Frame, x: u32, y: u32, background: [u8; 4]) -> Cell {
    std::array::from_fn(|row| {
        (0..ADVANCE)
            .filter(|&column| is_ink(frame, x + column, y + row as u32, background))
            .fold(0, |bits, column| bits | 1 << column)
    })
}

/// The first and last columns of `rect` with ink.
fn ink_columns(frame: &Frame, rect: Rect, background: [u8; 4]) -> Option<(u32, u32)> {
    let inked = |x: &u32| (rect.y..rect.y + rect.height).any(|y| is_ink(frame, *x, y, background));
    let first = (rect.x..rect.x + rect.width).find(inked)?;
    let last = (rect.x..rect.x + rect.width).rev().find(inked)?;
    Some((first, last))
}
