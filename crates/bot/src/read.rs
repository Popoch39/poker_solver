//! Frame → [`TableState`], by template matching on the client's fixed
//! layout.
//!
//! Every template comes from the client itself: its card sprites, and
//! glyphs, dealer button and turn marker cut out of tables drawn by its
//! renderer. Each region of [`LAYOUT`] is compared with the templates that
//! can appear there.

use std::collections::HashMap;

use nitro_local_client::{
    Frame, LAYOUT, Rect, SeatLayout, TEXT_HEIGHT, card_back, card_sprite, render,
};
use nitro_simulator::{Card, Decision, Position, Street, Suit, TableSeat, TableView, Value};

use crate::state::{Cards, SeatState, TableState};
use crate::text::{CHARSET, Font};

/// Why a frame could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error(
        "the frame is {width}×{height}, not the client's {}×{}",
        LAYOUT.window.width,
        LAYOUT.window.height
    )]
    Size { width: u32, height: u32 },
    #[error("unreadable {0}")]
    Unreadable(String),
    #[error("unexpected text in the {region}: {text:?}")]
    Unexpected { region: String, text: String },
}

/// A colour channel further than this from the template is a wrong pixel.
const PIXEL_TOLERANCE: u8 = 40;
/// Wrong pixels a region may have and still match a template: fewer than
/// one pixel of a card's rank glyph (drawn 4×4).
const MAX_PIXEL_ERRORS: usize = 8;

/// What a card slot shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Empty,
    Back,
    Card(Card),
}

/// What a seat's markers look like when drawn.
struct Markers {
    dealer: Frame,
    turn: Frame,
}

/// Reads tables drawn by the local client.
pub struct TableReader {
    /// By exact RGBA bytes.
    cards: HashMap<Vec<u8>, Slot>,
    font: Font,
    /// By seat.
    markers: [Markers; 3],
}

impl Default for TableReader {
    fn default() -> Self {
        Self::new()
    }
}

impl TableReader {
    /// Builds the templates from the client's renderer.
    pub fn new() -> Self {
        let mut cards = HashMap::new();
        for suit in Suit::suits() {
            for value in Value::values() {
                let card = Card::new(value, suit);
                cards.insert(card_sprite(card).rgba().to_vec(), Slot::Card(card));
            }
        }
        cards.insert(card_back().rgba().to_vec(), Slot::Back);
        let names = [""; 3].map(str::to_owned);
        let font = Font::from_status_line(&render(&empty_table(), &names, CHARSET));
        let markers = std::array::from_fn(|seat| {
            let table = TableView {
                button: seat,
                to_act: Some(seat),
                ..empty_table()
            };
            let frame = render(&table, &names, "");
            let layout = LAYOUT.seats[seat];
            Markers {
                dealer: frame.crop(layout.dealer),
                turn: frame.crop(layout.turn),
            }
        });
        Self {
            cards,
            font,
            markers,
        }
    }

    /// The table drawn in `frame`, a capture of the client's whole window.
    pub fn read(&self, frame: &Frame) -> Result<TableState, ReadError> {
        let (width, height) = (frame.width(), frame.height());
        if (width, height) != (LAYOUT.window.width, LAYOUT.window.height) {
            return Err(ReadError::Size { width, height });
        }
        let (hand_number, level, small_blind, big_blind) = self.info(frame)?;
        let board = LAYOUT
            .board
            .iter()
            .map_while(|&rect| match self.slot(frame, rect) {
                Slot::Card(card) => Some(card),
                Slot::Empty | Slot::Back => None,
            })
            .collect();
        let text = self.centered(frame, LAYOUT.pot, "pot")?;
        let pot = match text.strip_prefix("POT ") {
            Some(pot) => amount(pot).ok_or_else(|| unexpected("pot", &text))?,
            None if text.is_empty() => 0.0,
            None => return Err(unexpected("pot", &text)),
        };
        let button = match self.marked(frame, |layout| layout.dealer, |m| &m.dealer)[..] {
            [seat] => seat,
            _ => return Err(ReadError::Unreadable("dealer button".to_owned())),
        };
        let to_act = match self.marked(frame, |layout| layout.turn, |m| &m.turn)[..] {
            [] => None,
            [seat] => Some(seat),
            _ => return Err(ReadError::Unreadable("turn marker".to_owned())),
        };
        let (legal, to_call) = self.buttons(frame)?;
        let seats = [0, 1, 2].map(|seat| self.seat(frame, seat));
        let [Ok(first), Ok(second), Ok(third)] = seats else {
            return Err(seats.into_iter().find_map(Result::err).expect("an error"));
        };
        Ok(TableState {
            hand_number,
            level,
            small_blind,
            big_blind,
            button,
            board,
            pot,
            to_act,
            legal,
            to_call,
            seats: [first, second, third],
        })
    }

    /// `HAND 3  LEVEL 1  BLINDS 10/20`, without the hand before the first.
    fn info(&self, frame: &Frame) -> Result<(u32, u32, f32, f32), ReadError> {
        let text = self
            .font
            .left(frame, LAYOUT.info)
            .ok_or_else(|| ReadError::Unreadable("info line".to_owned()))?;
        let parse = || {
            let fields: Vec<&str> = text.split("  ").collect();
            let (hand_number, level, blinds) = match fields[..] {
                [hand, level, blinds] => (hand.strip_prefix("HAND ")?.parse().ok()?, level, blinds),
                [level, blinds] => (0, level, blinds),
                _ => return None,
            };
            let level = level.strip_prefix("LEVEL ")?.parse().ok()?;
            let (small, big) = blinds.strip_prefix("BLINDS ")?.split_once('/')?;
            Some((hand_number, level, amount(small)?, amount(big)?))
        };
        parse().ok_or_else(|| unexpected("info line", &text))
    }

    fn seat(&self, frame: &Frame, seat: usize) -> Result<SeatState, ReadError> {
        let layout = LAYOUT.seats[seat];
        let cards = match layout.cards.map(|rect| self.slot(frame, rect)) {
            [Slot::Card(first), Slot::Card(second)] => Cards::Shown([first, second]),
            [Slot::Back, Slot::Back] => Cards::Hidden,
            [Slot::Empty, Slot::Empty] => Cards::None,
            _ => return Err(ReadError::Unreadable(format!("seat {seat} cards"))),
        };

        let region = format!("seat {seat} name");
        let text = self.centered(frame, layout.name, &region)?;
        let position = match text.rsplit_once(' ').map_or("", |(_, last)| last) {
            "BTN" => Some(Position::Button),
            "SB" => Some(Position::SmallBlind),
            "BB" => Some(Position::BigBlind),
            _ => None,
        };

        let region = format!("seat {seat} stack");
        let text = self.centered(frame, layout.stack, &region)?;
        let (mut stack, mut all_in, mut place) = (0.0, false, None);
        if text == "ALL-IN" {
            all_in = true;
        } else if let Some(n) = text.strip_prefix("OUT ") {
            place = Some(ordinal(n).ok_or_else(|| unexpected(&region, &text))?);
        } else {
            stack = amount(&text).ok_or_else(|| unexpected(&region, &text))?;
        }

        let region = format!("seat {seat} bet");
        let text = self.centered(frame, layout.bet, &region)?;
        let (mut street_bet, mut won) = (0.0, 0.0);
        if let Some(n) = text.strip_prefix("WINS ") {
            won = amount(n).ok_or_else(|| unexpected(&region, &text))?;
        } else if !text.is_empty() {
            street_bet = amount(&text).ok_or_else(|| unexpected(&region, &text))?;
        }

        Ok(SeatState {
            position,
            stack,
            all_in,
            place,
            street_bet,
            won,
            cards,
        })
    }

    /// The hero's buttons, and the amount on the call button.
    fn buttons(&self, frame: &Frame) -> Result<(Vec<Decision>, f32), ReadError> {
        let mut legal = Vec::new();
        let mut to_call = 0.0;
        for decision in Decision::ALL {
            let button = LAYOUT.button(decision);
            let label = Rect::new(
                button.x,
                button.y + (button.height - TEXT_HEIGHT) / 2,
                button.width,
                TEXT_HEIGHT,
            );
            let region = format!("{decision:?} button");
            let text = self.centered(frame, label, &region)?;
            if text.is_empty() {
                continue;
            }
            let fine = match decision {
                Decision::Fold => text == "FOLD",
                Decision::Call if text == "CHECK" => true,
                Decision::Call => match text.strip_prefix("CALL ").and_then(amount) {
                    Some(amount) => {
                        to_call = amount;
                        true
                    }
                    None => false,
                },
                Decision::AllIn => text.strip_prefix("ALL-IN ").and_then(amount).is_some(),
            };
            if !fine {
                return Err(unexpected(&region, &text));
            }
            legal.push(decision);
        }
        Ok((legal, to_call))
    }

    /// The seats whose marker (as `region` and `template` pick it) is drawn.
    fn marked(
        &self,
        frame: &Frame,
        region: fn(&SeatLayout) -> Rect,
        template: fn(&Markers) -> &Frame,
    ) -> Vec<usize> {
        (0..3)
            .filter(|&seat| {
                let crop = frame.crop(region(&LAYOUT.seats[seat]));
                matches(crop.rgba(), template(&self.markers[seat]).rgba())
            })
            .collect()
    }

    fn slot(&self, frame: &Frame, rect: Rect) -> Slot {
        let crop = frame.crop(rect);
        if let Some(&slot) = self.cards.get(crop.rgba()) {
            return slot;
        }
        self.cards
            .iter()
            .find(|(template, _)| matches(crop.rgba(), template))
            .map_or(Slot::Empty, |(_, &slot)| slot)
    }

    fn centered(&self, frame: &Frame, rect: Rect, region: &str) -> Result<String, ReadError> {
        self.font
            .centered(frame, rect)
            .ok_or_else(|| ReadError::Unreadable(format!("text in the {region}")))
    }
}

/// Whether two same-sized RGBA images differ by at most
/// [`MAX_PIXEL_ERRORS`] pixels.
fn matches(image: &[u8], template: &[u8]) -> bool {
    let (image, _) = image.as_chunks::<4>();
    let (template, _) = template.as_chunks::<4>();
    // Stops at the first pixel too many: most templates differ at once.
    image
        .iter()
        .zip(template)
        .filter(|(a, b)| {
            a.iter()
                .zip(*b)
                .any(|(a, b)| a.abs_diff(*b) > PIXEL_TOLERANCE)
        })
        .nth(MAX_PIXEL_ERRORS)
        .is_none()
}

fn unexpected(region: &str, text: &str) -> ReadError {
    ReadError::Unexpected {
        region: region.to_owned(),
        text: text.to_owned(),
    }
}

/// `1ST`, `2ND`, `3RD`, `4TH`…
fn ordinal(text: &str) -> Option<u8> {
    let digits = text.trim_end_matches(char::is_alphabetic);
    let place: u8 = digits.parse().ok()?;
    let suffix = match place {
        1 => "ST",
        2 => "ND",
        3 => "RD",
        _ => "TH",
    };
    (text[digits.len()..] == *suffix).then_some(place)
}

/// Chips as the client writes them: digits, with one decimal at most.
fn amount(text: &str) -> Option<f32> {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let valid = match text.split_once('.') {
        Some((whole, fraction)) => digits(whole) && fraction.len() == 1 && digits(fraction),
        None => digits(text),
    };
    valid.then(|| text.parse().ok()).flatten()
}

/// A table with nothing on it, to draw templates on.
fn empty_table() -> TableView {
    let seat = TableSeat {
        stack: 0.0,
        position: None,
        street_bet: 0.0,
        in_hand: false,
        all_in: false,
        hole_cards: None,
        won: 0.0,
        place: None,
    };
    TableView {
        observer: 0,
        hand_number: 0,
        level: 1,
        small_blind: 10.0,
        big_blind: 20.0,
        button: 0,
        street: Street::Preflop,
        board: Vec::new(),
        pot: 0.0,
        to_act: None,
        legal: Vec::new(),
        to_call: 0.0,
        hand_over: false,
        seats: [seat; 3],
    }
}
