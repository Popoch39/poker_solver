//! Where everything is drawn, in frame pixels from the window's top-left
//! corner.
//!
//! The layout never moves: the clicker bot reads cards, stacks and the pot
//! from these regions and clicks these buttons.

use nitro_simulator::Decision;

/// A rectangle of the frame, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, x: u32, y: u32) -> bool {
        (self.x..self.x + self.width).contains(&x) && (self.y..self.y + self.height).contains(&y)
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }

    /// The pixel at the centre, where a click lands safely.
    pub fn center(&self) -> (u32, u32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

/// Every region of the frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    /// The whole frame: the window's fixed inner size.
    pub window: Rect,
    /// Hand number, level and blinds.
    pub info: Rect,
    /// The last thing that happened.
    pub status: Rect,
    pub pot: Rect,
    pub board: [Rect; 5],
    /// By table seat; the hero is seat [`crate::HERO`], at the bottom.
    pub seats: [SeatLayout; 3],
    pub fold: Rect,
    pub call: Rect,
    pub all_in: Rect,
    pub min_raise: Rect,
}

/// The regions of one seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatLayout {
    pub cards: [Rect; 2],
    /// Name and position (BTN, SB, BB).
    pub name: Rect,
    /// Chips behind, `ALL-IN`, or the place once eliminated.
    pub stack: Rect,
    /// Chips put in on this street, or `WINS n` once the hand is over.
    pub bet: Rect,
    /// The dealer button, drawn on the button's seat only.
    pub dealer: Rect,
    /// Lit while this seat must act.
    pub turn: Rect,
}

pub const CARD_WIDTH: u32 = 60;
pub const CARD_HEIGHT: u32 = 84;
/// Height of a line of text.
pub const TEXT_HEIGHT: u32 = 14;

const fn card(x: u32, y: u32) -> Rect {
    Rect::new(x, y, CARD_WIDTH, CARD_HEIGHT)
}

const fn line(x: u32, y: u32, width: u32) -> Rect {
    Rect::new(x, y, width, TEXT_HEIGHT)
}

const fn button(x: u32) -> Rect {
    Rect::new(x, 540, 160, 56)
}

const BOARD_X: u32 = 310;
const BOARD_Y: u32 = 176;

/// The one layout of the client.
pub const LAYOUT: Layout = Layout {
    window: Rect::new(0, 0, 960, 640),
    info: line(16, 16, 928),
    status: line(16, 490, 928),
    pot: line(BOARD_X, 272, 340),
    board: [
        card(BOARD_X, BOARD_Y),
        card(BOARD_X + 70, BOARD_Y),
        card(BOARD_X + 140, BOARD_Y),
        card(BOARD_X + 210, BOARD_Y),
        card(BOARD_X + 280, BOARD_Y),
    ],
    seats: [
        SeatLayout {
            cards: [card(415, 330), card(485, 330)],
            name: line(390, 424, 180),
            stack: line(390, 444, 180),
            bet: line(390, 300, 180),
            dealer: Rect::new(555, 336, 24, 24),
            turn: Rect::new(390, 462, 180, 4),
        },
        SeatLayout {
            cards: [card(140, 70), card(210, 70)],
            name: line(110, 160, 180),
            stack: line(110, 180, 180),
            bet: line(110, 216, 180),
            dealer: Rect::new(280, 76, 24, 24),
            turn: Rect::new(110, 198, 180, 4),
        },
        SeatLayout {
            cards: [card(690, 70), card(760, 70)],
            name: line(670, 160, 180),
            stack: line(670, 180, 180),
            bet: line(670, 216, 180),
            dealer: Rect::new(656, 76, 24, 24),
            turn: Rect::new(670, 198, 180, 4),
        },
    ],
    fold: button(220),
    call: button(400),
    all_in: button(580),
    min_raise: button(760),
};

impl Layout {
    /// The button that takes `decision`.
    pub fn button(&self, decision: Decision) -> Rect {
        match decision {
            Decision::Fold => self.fold,
            Decision::Call => self.call,
            Decision::AllIn => self.all_in,
            Decision::MinRaise => self.min_raise,
        }
    }
}
