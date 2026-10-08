//! Draws a [`TableView`] into a [`Frame`]: the same pixels on screen and
//! offscreen.
//!
//! Everything is drawn on the CPU with integer coordinates, cards and text
//! without anti-aliasing, so a given table always gives the same frame and
//! a card or a string always the same pixels wherever it is drawn.

use std::fmt;
use std::io;
use std::path::Path;

use nitro_simulator::{Card, Decision, Position, Suit, TableView};
use tiny_skia::{Color, FillRule, IntRect, Paint, PathBuilder, Pixmap, Transform};

use crate::font::{self, GLYPH_SPACING, GLYPH_WIDTH, SUIT_SIZE};
use crate::layout::{CARD_HEIGHT, CARD_WIDTH, LAYOUT, Rect, TEXT_HEIGHT};

/// An opaque RGBA image.
#[derive(Clone, PartialEq)]
pub struct Frame {
    pixmap: Pixmap,
}

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frame({}×{})", self.width(), self.height())
    }
}

impl Frame {
    fn filled(width: u32, height: u32, color: Rgb) -> Self {
        let mut pixmap = Pixmap::new(width, height).expect("a non-empty frame");
        pixmap.fill(color.into());
        Self { pixmap }
    }

    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }

    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }

    /// RGBA bytes, row by row from the top-left corner (every pixel is
    /// opaque).
    pub fn rgba(&self) -> &[u8] {
        self.pixmap.data()
    }

    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width() + x) * 4) as usize;
        self.rgba()[i..i + 4].try_into().expect("four bytes")
    }

    /// The pixels inside `rect`.
    pub fn crop(&self, rect: Rect) -> Frame {
        let area = IntRect::from_xywh(rect.x as i32, rect.y as i32, rect.width, rect.height)
            .expect("a non-empty rectangle");
        Self {
            pixmap: self.pixmap.clone_rect(area).expect("inside the frame"),
        }
    }

    pub fn encode_png(&self) -> Vec<u8> {
        self.pixmap
            .encode_png()
            .expect("an in-memory PNG always encodes")
    }

    pub fn decode_png(bytes: &[u8]) -> io::Result<Frame> {
        Pixmap::decode_png(bytes)
            .map(|pixmap| Self { pixmap })
            .map_err(io::Error::other)
    }

    pub fn save_png(&self, path: impl AsRef<Path>) -> io::Result<()> {
        std::fs::write(path, self.encode_png())
    }

    fn fill(&mut self, rect: Rect, color: Rgb) {
        let rect = tiny_skia::Rect::from_xywh(
            rect.x as f32,
            rect.y as f32,
            rect.width as f32,
            rect.height as f32,
        )
        .expect("a non-empty rectangle");
        self.pixmap
            .fill_rect(rect, &paint(color, false), Transform::identity(), None);
    }

    /// Copies `image` with its top-left corner at (`x`, `y`).
    fn blit(&mut self, image: &Frame, x: u32, y: u32) {
        let width = self.width() as usize;
        let row = image.width() as usize * 4;
        let data = self.pixmap.data_mut();
        for (dy, line) in image.rgba().chunks_exact(row).enumerate() {
            let start = ((y as usize + dy) * width + x as usize) * 4;
            data[start..start + row].copy_from_slice(line);
        }
    }

    /// Draws `bitmap` rows (bit `columns − 1` leftmost), each bit a
    /// `scale`-wide square.
    fn bitmap(&mut self, rows: &[u8], columns: u32, x: u32, y: u32, scale: u32, color: Rgb) {
        for (dy, bits) in rows.iter().enumerate() {
            for dx in 0..columns {
                if bits >> (columns - 1 - dx) & 1 == 1 {
                    let rect = Rect::new(x + dx * scale, y + dy as u32 * scale, scale, scale);
                    self.fill(rect, color);
                }
            }
        }
    }

    fn text(&mut self, text: &str, x: u32, y: u32, scale: u32, color: Rgb) {
        let advance = (GLYPH_WIDTH + GLYPH_SPACING) * scale;
        for (i, c) in text.chars().enumerate() {
            let x = x + i as u32 * advance;
            self.bitmap(&font::glyph(c), GLYPH_WIDTH, x, y, scale, color);
        }
    }

    /// `text` on one line of `rect`, centred, clipped to whole glyphs.
    fn label(&mut self, rect: Rect, text: &str, color: Rgb) {
        let text = fit(text, rect.width);
        let x = rect.x + (rect.width - font::text_width(&text, TEXT_SCALE)) / 2;
        self.text(&text, x, rect.y, TEXT_SCALE, color);
    }

    /// `text` on one line of `rect`, left-aligned, clipped to whole glyphs.
    fn line(&mut self, rect: Rect, text: &str, color: Rgb) {
        self.text(&fit(text, rect.width), rect.x, rect.y, TEXT_SCALE, color);
    }

    fn rounded_rect(&mut self, rect: Rect, radius: f32, color: Rgb) {
        let (x, y) = (rect.x as f32, rect.y as f32);
        let (right, bottom) = (x + rect.width as f32, y + rect.height as f32);
        let mut path = PathBuilder::new();
        path.move_to(x + radius, y);
        path.line_to(right - radius, y);
        path.quad_to(right, y, right, y + radius);
        path.line_to(right, bottom - radius);
        path.quad_to(right, bottom, right - radius, bottom);
        path.line_to(x + radius, bottom);
        path.quad_to(x, bottom, x, bottom - radius);
        path.line_to(x, y + radius);
        path.quad_to(x, y, x + radius, y);
        path.close();
        let path = path.finish().expect("a closed path");
        self.pixmap.fill_path(
            &path,
            &paint(color, true),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    fn disc(&mut self, rect: Rect, color: Rgb) {
        let (cx, cy) = rect.center();
        let path = PathBuilder::from_circle(cx as f32, cy as f32, rect.width as f32 / 2.0)
            .expect("a positive radius");
        self.pixmap.fill_path(
            &path,
            &paint(color, true),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// Text is drawn at twice the font size: 10×14-pixel glyphs.
const TEXT_SCALE: u32 = 2;

/// The longest prefix of `text` that fits in `width` pixels.
fn fit(text: &str, width: u32) -> String {
    let advance = (GLYPH_WIDTH + GLYPH_SPACING) * TEXT_SCALE;
    let max = ((width + GLYPH_SPACING * TEXT_SCALE) / advance) as usize;
    text.chars().take(max).collect()
}

#[derive(Debug, Clone, Copy)]
struct Rgb(u8, u8, u8);

impl From<Rgb> for Color {
    fn from(Rgb(r, g, b): Rgb) -> Self {
        Color::from_rgba8(r, g, b, 255)
    }
}

fn paint(color: Rgb, anti_alias: bool) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(color.into());
    paint.anti_alias = anti_alias;
    paint
}

const BACKGROUND: Rgb = Rgb(24, 28, 34);
const RAIL: Rgb = Rgb(70, 46, 26);
const FELT: Rgb = Rgb(22, 101, 56);
const TEXT: Rgb = Rgb(235, 235, 235);
const TEXT_DIM: Rgb = Rgb(120, 126, 132);
const TURN: Rgb = Rgb(240, 200, 40);
const CARD_EDGE: Rgb = Rgb(60, 60, 60);
const CARD_FACE: Rgb = Rgb(250, 250, 250);
const BACK_DARK: Rgb = Rgb(120, 20, 30);
const BACK_LIGHT: Rgb = Rgb(160, 40, 50);
const DEALER: Rgb = Rgb(250, 250, 250);
const DEALER_TEXT: Rgb = Rgb(20, 20, 20);
const FOLD: Rgb = Rgb(170, 50, 50);
const CALL: Rgb = Rgb(40, 110, 170);
const ALL_IN: Rgb = Rgb(200, 120, 30);
const MIN_RAISE: Rgb = Rgb(120, 70, 160);

/// A four-colour deck: no two suits share a colour.
fn suit_color(suit: Suit) -> Rgb {
    match suit {
        Suit::Spade => Rgb(20, 20, 20),
        Suit::Heart => Rgb(200, 30, 30),
        Suit::Diamond => Rgb(30, 80, 200),
        Suit::Club => Rgb(20, 130, 40),
    }
}

fn blank_card(face: Rgb) -> Frame {
    let mut card = Frame::filled(CARD_WIDTH, CARD_HEIGHT, CARD_EDGE);
    card.fill(Rect::new(1, 1, CARD_WIDTH - 2, CARD_HEIGHT - 2), face);
    card
}

/// How a face-up card looks, wherever it is drawn.
pub fn card_sprite(card: Card) -> Frame {
    let mut sprite = blank_card(CARD_FACE);
    let color = suit_color(card.suit);
    let rank = font::glyph(card.value.to_char());
    sprite.bitmap(&rank, GLYPH_WIDTH, 6, 6, 4, color);
    let suit = font::suit_symbol(card.suit);
    sprite.bitmap(&suit, SUIT_SIZE, 32, 8, 3, color);
    sprite.bitmap(&suit, SUIT_SIZE, 16, 46, 4, color);
    sprite
}

/// How a face-down card looks, wherever it is drawn.
pub fn card_back() -> Frame {
    let mut sprite = blank_card(BACK_DARK);
    let square = 6;
    for row in 0..(CARD_HEIGHT - 8) / square {
        for column in 0..(CARD_WIDTH - 8) / square {
            if (row + column) % 2 == 0 {
                let rect = Rect::new(4 + column * square, 4 + row * square, square, square);
                sprite.fill(rect, BACK_LIGHT);
            }
        }
    }
    sprite
}

/// Chips as shown on the table: whole chips, or one decimal for the
/// fractions a split pot leaves.
pub fn chips(amount: f32) -> String {
    if amount.fract() == 0.0 {
        format!("{amount:.0}")
    } else {
        format!("{amount:.1}")
    }
}

pub(crate) fn ordinal(place: u8) -> String {
    match place {
        1 => "1ST".to_owned(),
        2 => "2ND".to_owned(),
        3 => "3RD".to_owned(),
        n => format!("{n}TH"),
    }
}

fn position_label(position: Position) -> &'static str {
    match position {
        Position::Button => "BTN",
        Position::SmallBlind => "SB",
        Position::BigBlind => "BB",
    }
}

/// The label of the button that takes `decision`.
fn button_label(view: &TableView, decision: Decision) -> String {
    match decision {
        Decision::Fold => "FOLD".to_owned(),
        Decision::Call if view.to_call == 0.0 => "CHECK".to_owned(),
        Decision::Call => format!("CALL {}", chips(view.to_call)),
        Decision::AllIn => format!("ALL-IN {}", chips(view.seats[view.observer].stack)),
        Decision::MinRaise => format!("RAISE {}", chips(2.0 * view.big_blind)),
    }
}

/// The whole table as `view.observer` sees it, with the seats' `names` and
/// a `status` line.
pub fn render(view: &TableView, names: &[String; 3], status: &str) -> Frame {
    let window = LAYOUT.window;
    let mut frame = Frame::filled(window.width, window.height, BACKGROUND);
    frame.rounded_rect(Rect::new(48, 38, 864, 384), 150.0, RAIL);
    frame.rounded_rect(Rect::new(60, 50, 840, 360), 140.0, FELT);

    let info = if view.hand_number == 0 {
        format!(
            "LEVEL {}  BLINDS {}/{}",
            view.level,
            chips(view.small_blind),
            chips(view.big_blind)
        )
    } else {
        format!(
            "HAND {}  LEVEL {}  BLINDS {}/{}",
            view.hand_number,
            view.level,
            chips(view.small_blind),
            chips(view.big_blind)
        )
    };
    frame.line(LAYOUT.info, &info, TEXT);
    frame.line(LAYOUT.status, status, TEXT);

    for (card, rect) in view.board.iter().zip(LAYOUT.board) {
        frame.blit(&card_sprite(*card), rect.x, rect.y);
    }
    if view.pot > 0.0 {
        frame.label(LAYOUT.pot, &format!("POT {}", chips(view.pot)), TEXT);
    }

    for (seat, (shown, layout)) in view.seats.iter().zip(LAYOUT.seats).enumerate() {
        if shown.in_hand {
            for (i, rect) in layout.cards.iter().enumerate() {
                let sprite = match shown.hole_cards {
                    Some(cards) => card_sprite(cards[i]),
                    None => card_back(),
                };
                frame.blit(&sprite, rect.x, rect.y);
            }
        }
        let name = match shown.position {
            Some(position) => format!("{} {}", names[seat], position_label(position)),
            None => names[seat].clone(),
        };
        let dealt_out = view.hand_number > 0 && !shown.in_hand;
        frame.label(layout.name, &name, if dealt_out { TEXT_DIM } else { TEXT });
        let stack = match shown.place {
            Some(place) if shown.stack == 0.0 => format!("OUT {}", ordinal(place)),
            _ if shown.all_in && shown.stack == 0.0 => "ALL-IN".to_owned(),
            _ => chips(shown.stack),
        };
        frame.label(layout.stack, &stack, TEXT);
        if view.hand_over && shown.won > 0.0 {
            frame.label(layout.bet, &format!("WINS {}", chips(shown.won)), TURN);
        } else if shown.street_bet > 0.0 {
            frame.label(layout.bet, &chips(shown.street_bet), TEXT);
        }
        if view.button == seat {
            frame.disc(layout.dealer, DEALER);
            let (cx, cy) = layout.dealer.center();
            frame.text("D", cx - 5, cy - 7, TEXT_SCALE, DEALER_TEXT);
        }
        if view.to_act == Some(seat) {
            frame.fill(layout.turn, TURN);
        }
    }

    for decision in &view.legal {
        let rect = LAYOUT.button(*decision);
        let color = match decision {
            Decision::Fold => FOLD,
            Decision::Call => CALL,
            Decision::AllIn => ALL_IN,
            Decision::MinRaise => MIN_RAISE,
        };
        frame.fill(rect, color);
        let text = Rect::new(
            rect.x,
            rect.y + (rect.height - TEXT_HEIGHT) / 2,
            rect.width,
            TEXT_HEIGHT,
        );
        frame.label(text, &button_label(view, *decision), TEXT);
    }
    frame
}
