//! Simulated hands written down as hand histories, the parser's structured
//! [`Hand`], so that they export to Open Hand History with
//! [`nitro_hh::to_ohh`] and read back with [`nitro_hh::from_ohh`].
//!
//! The arena's own Open Hand History historian is not used: it numbers the
//! seats of a heads-up table 1 and 2 whatever their table seats, stamps
//! hands with the wall clock and leaves split pots in fractions of chips.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::DateTime;
use nitro_hh::{Action, ActionKind, Hand, Player, Street};
use rs_poker::arena::action::{
    Action as ArenaAction, AgentAction, ForcedBetType, PlayedActionPayload,
};
use rs_poker::arena::game_state::Round;
use rs_poker::arena::{GameState, Historian, HistorianError};
use rs_poker::core::Card;

/// Table name written in every simulated hand.
pub(crate) const TABLE_NAME: &str = "Expresso Nitro (simulated)";

/// The arena actions of one hand that a hand history needs, in order.
pub(crate) type HandLog = Arc<Mutex<Vec<ArenaAction>>>;

/// Keeps the blinds, the decisions and the cards dealt to the board.
pub(crate) struct LogHistorian(pub(crate) HandLog);

#[async_trait]
impl Historian for LogHistorian {
    async fn record_action(
        &mut self,
        _id: u128,
        _state: &GameState,
        action: &ArenaAction,
    ) -> Result<(), HistorianError> {
        if matches!(
            action,
            ArenaAction::ForcedBet(_)
                | ArenaAction::PlayedAction(_)
                | ArenaAction::FailedAction(_)
                | ArenaAction::DealCommunity(_)
        ) {
            self.0
                .lock()
                .map_err(|_| HistorianError::UnableToRecordAction)?
                .push(action.clone());
        }
        Ok(())
    }
}

/// One finished hand, as the game knows it.
pub(crate) struct PlayedHand<'a> {
    pub(crate) number: u32,
    pub(crate) level: u32,
    /// Table seat of the button.
    pub(crate) button: usize,
    /// Table seat of each arena index.
    pub(crate) table: &'a [usize],
    /// Name of each arena index's player.
    pub(crate) names: Vec<String>,
    /// Stacks before the blinds, by arena index.
    pub(crate) starting_stacks: &'a [f32],
    /// The arena state once the hand is over and settled in whole chips.
    pub(crate) state: &'a GameState,
    pub(crate) hole_cards: Vec<[Card; 2]>,
    pub(crate) shown_down: bool,
    pub(crate) log: &'a [ArenaAction],
}

/// Writes `played` down as `hero`, the table seat whose cards are dealt
/// face up (no one's when `None`). Table seats 0 to 2 become seats 1 to 3.
///
/// Simulated hands have no real date: they all start at the Unix epoch.
pub(crate) fn write(played: &PlayedHand, hero: Option<usize>) -> Hand {
    let seat = |i: usize| played.table[i] as u8 + 1;
    let state = played.state;
    let mut actions = Vec::new();
    let mut board = Vec::new();
    let mut put_in = vec![0u32; played.table.len()];
    for action in played.log {
        let (i, street, kind, added) = match action {
            ArenaAction::ForcedBet(bet) => {
                let added = chips(bet.bet);
                let kind = match bet.forced_bet_type {
                    ForcedBetType::SmallBlind => ActionKind::SmallBlind(added),
                    ForcedBetType::BigBlind => ActionKind::BigBlind(added),
                    ForcedBetType::Ante => unreachable!("Expresso Nitro has no ante"),
                };
                (bet.idx, Street::Preflop, kind, added)
            }
            ArenaAction::PlayedAction(payload) => decision(payload),
            // The arena records what it played instead of an illegal action
            // (a fold with nothing to call becomes a check).
            ArenaAction::FailedAction(failed) => decision(&failed.result),
            ArenaAction::DealCommunity(card) => {
                board.push(*card);
                continue;
            }
            _ => continue,
        };
        put_in[i] += added;
        actions.push(Action {
            street,
            seat: seat(i),
            kind,
            all_in: added > 0 && chips(played.starting_stacks[i]) == put_in[i],
        });
    }

    let in_hand = |i: usize| state.player_active.get(i) || state.player_all_in.get(i);
    let players = (0..played.table.len())
        .map(|i| {
            let is_hero = hero == Some(played.table[i]);
            let showed = played.shown_down && in_hand(i);
            let start = chips(played.starting_stacks[i]);
            Player {
                seat: seat(i),
                name: played.names[i].clone(),
                stack: start,
                cards: (is_hero || showed).then_some(played.hole_cards[i]),
                showed,
                collected: chips(state.stacks[i]) + put_in[i] - start,
            }
        })
        .collect();

    Hand {
        game_number: played.number.to_string(),
        started_at: DateTime::UNIX_EPOCH,
        tournament: None,
        level: Some(played.level),
        table_name: TABLE_NAME.into(),
        table_size: 3,
        button: played.button as u8 + 1,
        small_blind: chips(state.small_blind),
        big_blind: chips(state.big_blind),
        players,
        hero: hero
            .filter(|h| played.table.contains(h))
            .map(|h| h as u8 + 1),
        actions,
        board,
    }
}

/// A decision of the arena as the parser writes it (amounts are the chips
/// it adds, except a raise, which gives the player's total on the street),
/// with the chips it adds.
fn decision(payload: &PlayedActionPayload) -> (usize, Street, ActionKind, u32) {
    let added = chips(payload.final_player_bet - payload.starting_player_bet);
    let kind = match payload.action {
        AgentAction::Fold => ActionKind::Fold,
        _ if added == 0 => ActionKind::Check,
        _ if payload.final_bet > payload.starting_bet && payload.starting_bet == 0.0 => {
            ActionKind::Bet(added)
        }
        _ if payload.final_bet > payload.starting_bet => ActionKind::Raise {
            to: chips(payload.final_player_bet),
        },
        _ => ActionKind::Call(added),
    };
    let street = match payload.round {
        Round::Flop => Street::Flop,
        Round::Turn => Street::Turn,
        Round::River => Street::River,
        _ => Street::Preflop,
    };
    (payload.idx, street, kind, added)
}

/// A chip amount of the simulator, whole since split pots are settled.
fn chips(amount: f32) -> u32 {
    debug_assert_eq!(amount.fract(), 0.0, "{amount} is not whole chips");
    amount.round() as u32
}
