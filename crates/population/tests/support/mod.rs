//! Hands built by hand for the population-model tests.
//!
//! Seats are fixed so that a seat number reads as a position: 1 is the BTN,
//! 2 the SB and 3 the BB. Heads-up, seat 2 is both the button and the SB.
//! Blinds are 10/20 and every player but the account owner is named after
//! their seat.

use chrono::DateTime;
use nitro_hh::{Action, ActionKind, Card, Hand, Player, Street};

pub const BTN: u8 = 1;
pub const SB: u8 = 2;
pub const BB: u8 = 3;

/// Pseudo of the account owner set by [`HandBuilder::hero`].
pub const HERO: &str = "Zorglub";

const SMALL_BLIND: u32 = 10;
const BIG_BLIND: u32 = 20;

pub struct HandBuilder {
    hand: Hand,
    street: Street,
    /// Per seat: chips put in on the current street, and in the whole hand.
    chips: Vec<(u8, u32, u32)>,
}

impl HandBuilder {
    /// A 3-handed hand with the BTN, SB and BB stacks given in chips; the
    /// blinds are posted (all-in when a stack is too short).
    pub fn three_handed(stacks: [u32; 3]) -> HandBuilder {
        HandBuilder::new(BTN, &[(BTN, stacks[0]), (SB, stacks[1]), (BB, stacks[2])])
    }

    /// A heads-up hand: the button posts the small blind.
    pub fn heads_up(sb: u32, bb: u32) -> HandBuilder {
        HandBuilder::new(SB, &[(SB, sb), (BB, bb)])
    }

    fn new(button: u8, seats: &[(u8, u32)]) -> HandBuilder {
        let hand = Hand {
            game_number: "1-1-1".into(),
            started_at: DateTime::UNIX_EPOCH,
            tournament: None,
            level: Some(1),
            table_name: "Expresso Nitro(1)#0".into(),
            table_size: 3,
            button,
            small_blind: SMALL_BLIND,
            big_blind: BIG_BLIND,
            players: seats
                .iter()
                .map(|&(seat, stack)| Player {
                    seat,
                    name: format!("villain {seat}"),
                    stack,
                    cards: None,
                    showed: false,
                    collected: 0,
                })
                .collect(),
            hero: None,
            actions: Vec::new(),
            board: Vec::new(),
        };
        let mut builder = HandBuilder {
            hand,
            street: Street::Preflop,
            chips: seats.iter().map(|&(seat, _)| (seat, 0, 0)).collect(),
        };
        let (sb, bb) = (seats[seats.len() - 2].0, seats[seats.len() - 1].0);
        let posted = builder.put_in(sb, SMALL_BLIND);
        builder.record(sb, ActionKind::SmallBlind(posted));
        let posted = builder.put_in(bb, BIG_BLIND);
        builder.record(bb, ActionKind::BigBlind(posted));
        builder
    }

    pub fn fold(mut self, seat: u8) -> HandBuilder {
        self.record(seat, ActionKind::Fold);
        self
    }

    pub fn check(mut self, seat: u8) -> HandBuilder {
        self.record(seat, ActionKind::Check);
        self
    }

    /// Calls the highest bet of the street, or what is left of the stack.
    pub fn call(mut self, seat: u8) -> HandBuilder {
        let to_match = self
            .chips
            .iter()
            .map(|&(_, street, _)| street)
            .max()
            .unwrap();
        let added = self.put_in(seat, to_match - self.street_total(seat));
        self.record(seat, ActionKind::Call(added));
        self
    }

    /// Raises to a total of `to` chips on the street.
    pub fn raise_to(mut self, seat: u8, to: u32) -> HandBuilder {
        self.put_in(seat, to - self.street_total(seat));
        let to = self.street_total(seat);
        self.record(seat, ActionKind::Raise { to });
        self
    }

    /// Moves all-in with a raise.
    pub fn push(self, seat: u8) -> HandBuilder {
        let to = self.street_total(seat) + self.behind(seat);
        self.raise_to(seat, to)
    }

    /// Deals the next street: later actions belong to it.
    pub fn deal(mut self, street: Street) -> HandBuilder {
        self.street = street;
        for (_, on_street, _) in &mut self.chips {
            *on_street = 0;
        }
        self
    }

    /// The player at `seat` shows `cards` (e.g. `"Ah Kd"`) at showdown.
    pub fn show(mut self, seat: u8, cards: &str) -> HandBuilder {
        let player = self.player(seat);
        player.cards = Some(parse_cards(cards));
        player.showed = true;
        self
    }

    /// The account owner, named [`HERO`], sits at `seat` and is dealt `cards`.
    pub fn hero(self, seat: u8, cards: &str) -> HandBuilder {
        self.account_owner(seat, HERO, cards)
    }

    /// The account owner of the history is `name`, at `seat`, dealt `cards`.
    pub fn account_owner(mut self, seat: u8, name: &str, cards: &str) -> HandBuilder {
        let player = self.player(seat);
        player.name = name.into();
        player.cards = Some(parse_cards(cards));
        self.hand.hero = Some(seat);
        self
    }

    pub fn build(self) -> Hand {
        self.hand
    }

    fn player(&mut self, seat: u8) -> &mut Player {
        self.hand
            .players
            .iter_mut()
            .find(|p| p.seat == seat)
            .expect("the seat is dealt in")
    }

    fn entry(&mut self, seat: u8) -> &mut (u8, u32, u32) {
        self.chips.iter_mut().find(|(s, _, _)| *s == seat).unwrap()
    }

    fn street_total(&self, seat: u8) -> u32 {
        self.chips.iter().find(|(s, _, _)| *s == seat).unwrap().1
    }

    fn behind(&self, seat: u8) -> u32 {
        let stack = self
            .hand
            .players
            .iter()
            .find(|p| p.seat == seat)
            .unwrap()
            .stack;
        stack - self.chips.iter().find(|(s, _, _)| *s == seat).unwrap().2
    }

    /// Moves up to `chips` from the player's stack to the pot; returns what
    /// was actually put in.
    fn put_in(&mut self, seat: u8, chips: u32) -> u32 {
        let chips = chips.min(self.behind(seat));
        let entry = self.entry(seat);
        entry.1 += chips;
        entry.2 += chips;
        chips
    }

    fn record(&mut self, seat: u8, kind: ActionKind) {
        let all_in =
            !matches!(kind, ActionKind::Fold | ActionKind::Check) && self.behind(seat) == 0;
        self.hand.actions.push(Action {
            street: self.street,
            seat,
            kind,
            all_in,
        });
    }
}

fn parse_cards(text: &str) -> [Card; 2] {
    let cards: Vec<Card> = text
        .split(' ')
        .map(|c| Card::try_from(c).expect("a card such as Ah"))
        .collect();
    cards.try_into().expect("two cards")
}
