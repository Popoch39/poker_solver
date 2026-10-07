//! Conversion between [`Hand`] and Open Hand History, through `rs_poker`'s
//! OHH types so that the parser and the simulator share one format.

use std::collections::HashMap;
use std::fmt;

use rs_poker::open_hand_history::{
    Action as OhhAction, ActionObj, BetLimitObj, BetType, GameType, HandHistory, PlayerObj,
    PlayerWinsObj, PotObj, RoundObj, SpeedObj, SpeedType, TournamentFlag, TournamentInfoObj,
    TournamentType,
};

use crate::hand::{Action, ActionKind, BuyIn, Euros, Hand, Player, Street, TournamentInfo};
use crate::summary::Summary;

/// The OHH version of `rs_poker`'s types, which leave out the 1.4.8+ fields.
const SPEC_VERSION: &str = "1.4.7";
const SITE: &str = "Winamax";

/// Why an OHH hand cannot be read as a [`Hand`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OhhError(pub String);

impl fmt::Display for OhhError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OhhError {}

/// Converts a hand to Open Hand History. The tournament summary, when known,
/// adds the tournament start date and the level duration.
///
/// The pot holds every chip put in, including the part of a bet nobody
/// called, credited back to the bettor: that is what Winamax writes and
/// what `rs_poker`'s simulator records. [`from_ohh`] also reads hands whose
/// pots leave it out, as the OHH examples do.
pub fn to_ohh(hand: &Hand, summary: Option<&Summary>) -> HandHistory {
    let id_of = |seat: u8| {
        hand.players
            .iter()
            .position(|p| p.seat == seat)
            .expect("every action belongs to a seated player") as u64
    };

    let mut rounds = vec![RoundObj {
        id: 0,
        street: "Preflop".into(),
        cards: None,
        actions: Vec::new(),
    }];
    let mut on_street: HashMap<u8, u32> = HashMap::new();
    let dealt = |rounds: &mut Vec<RoundObj>| {
        if let Some(hero) = hand.hero
            && let Some(cards) = hand.players[id_of(hero) as usize].cards
        {
            push_action(&mut rounds[0], id_of(hero), OhhAction::DealtCards, 0, false).cards =
                Some(cards.to_vec());
        }
    };
    let mut hole_cards_dealt = false;
    for action in &hand.actions {
        let blind = matches!(
            action.kind,
            ActionKind::SmallBlind(_) | ActionKind::BigBlind(_)
        );
        if !blind && !hole_cards_dealt {
            dealt(&mut rounds);
            hole_cards_dealt = true;
        }
        if street_name(action.street) != rounds.last().expect("preflop").street {
            on_street.clear();
            rounds.push(street_round(hand, action.street, rounds.len()));
        }
        let put = on_street.entry(action.seat).or_default();
        let (kind, added) = match action.kind {
            ActionKind::SmallBlind(n) => (OhhAction::PostSmallBlind, n),
            ActionKind::BigBlind(n) => (OhhAction::PostBigBlind, n),
            ActionKind::Fold => (OhhAction::Fold, 0),
            ActionKind::Check => (OhhAction::Check, 0),
            ActionKind::Call(n) => (OhhAction::Call, n),
            ActionKind::Bet(n) => (OhhAction::Bet, n),
            ActionKind::Raise { to } => (OhhAction::Raise, to - *put),
        };
        *put += added;
        let round = rounds.last_mut().expect("preflop");
        push_action(round, id_of(action.seat), kind, added, action.all_in);
    }
    if !hole_cards_dealt {
        dealt(&mut rounds);
    }
    // Streets dealt after everyone was all-in have no action.
    for street in [Street::Flop, Street::Turn, Street::River] {
        let dealt = board_range(street).end <= hand.board.len();
        let present = rounds.iter().any(|r| r.street == street_name(street));
        if dealt && !present {
            rounds.push(street_round(hand, street, rounds.len()));
        }
    }
    let showers: Vec<&Player> = hand.players.iter().filter(|p| p.showed).collect();
    if !showers.is_empty() {
        let mut showdown = RoundObj {
            id: rounds.len() as u64,
            street: "Showdown".into(),
            cards: None,
            actions: Vec::new(),
        };
        for player in showers {
            push_action(
                &mut showdown,
                id_of(player.seat),
                OhhAction::ShowsCards,
                0,
                false,
            )
            .cards = player.cards.map(|c| c.to_vec());
        }
        rounds.push(showdown);
    }

    let pot: u32 = hand.players.iter().map(|p| p.collected).sum();
    let player_wins = hand
        .players
        .iter()
        .filter(|p| p.collected > 0)
        .map(|p| PlayerWinsObj {
            player_id: id_of(p.seat),
            win_amount: p.collected as f32,
            cashout_amount: None,
            cashout_fee: None,
            bonus_amount: None,
            contributed_rake: None,
        })
        .collect();

    HandHistory {
        spec_version: SPEC_VERSION.into(),
        site_name: SITE.into(),
        network_name: SITE.into(),
        internal_version: env!("CARGO_PKG_VERSION").into(),
        tournament: true,
        tournament_info: hand
            .tournament
            .as_ref()
            .map(|t| tournament_info(hand, t, summary)),
        game_number: hand.game_number.clone(),
        start_date_utc: Some(hand.started_at),
        table_name: hand.table_name.clone(),
        table_handle: None,
        table_skin: None,
        game_type: GameType::Holdem,
        bet_limit: Some(BetLimitObj {
            bet_type: BetType::NoLimit,
            bet_cap: 0.0,
        }),
        table_size: u64::from(hand.table_size),
        // Tournament amounts are chips; the money currency is in tournament_info.
        currency: String::new(),
        dealer_seat: u64::from(hand.button),
        small_blind_amount: hand.small_blind as f32,
        big_blind_amount: hand.big_blind as f32,
        ante_amount: 0.0,
        hero_player_id: hand.hero.map(id_of),
        players: hand
            .players
            .iter()
            .enumerate()
            .map(|(id, p)| PlayerObj {
                id: id as u64,
                seat: u64::from(p.seat),
                name: p.name.clone(),
                display: None,
                starting_stack: p.stack as f32,
                player_bounty: None,
                is_sitting_out: None,
            })
            .collect(),
        rounds,
        pots: vec![PotObj {
            number: 0,
            amount: pot as f32,
            rake: Some(0.0),
            jackpot: None,
            player_wins,
        }],
        tournament_bounties: None,
    }
}

fn tournament_info(
    hand: &Hand,
    info: &TournamentInfo,
    summary: Option<&Summary>,
) -> TournamentInfoObj {
    let euros = |amount: Euros| amount.cents() as f32 / 100.0;
    // A freezeout keeps every chip in play: the starting stack is the chips
    // at the table shared among the seats, whoever has busted since.
    let chips: u32 = hand.players.iter().map(|p| p.stack).sum();
    let level_duration = summary
        .zip(hand.level.and_then(|level| level.checked_sub(1)))
        .and_then(|(s, index)| s.levels.get(index as usize))
        .map_or(0, |l| l.duration.as_secs());
    TournamentInfoObj {
        tournament_number: info.id.clone(),
        name: info.name.clone(),
        start_date_utc: summary.map(|s| s.started_at),
        currency: "EUR".into(),
        buyin_amount: euros(info.buy_in.prize),
        fee_amount: euros(info.buy_in.rake),
        bounty_fee_amount: 0.0,
        initial_stack: u64::from(chips / u32::from(hand.table_size.max(1))),
        tournament_type: TournamentType::SingleTableTournament,
        flags: Some(vec![TournamentFlag::SitNGo, TournamentFlag::Lottery]),
        speed: SpeedObj {
            speed_type: SpeedType::HyperTurbo,
            round_time: level_duration,
        },
    }
}

fn street_name(street: Street) -> &'static str {
    match street {
        Street::Preflop => "Preflop",
        Street::Flop => "Flop",
        Street::Turn => "Turn",
        Street::River => "River",
    }
}

/// Where a street's new cards sit in the board.
fn board_range(street: Street) -> std::ops::Range<usize> {
    match street {
        Street::Preflop => 0..0,
        Street::Flop => 0..3,
        Street::Turn => 3..4,
        Street::River => 4..5,
    }
}

fn street_round(hand: &Hand, street: Street, id: usize) -> RoundObj {
    RoundObj {
        id: id as u64,
        street: street_name(street).into(),
        cards: hand.board.get(board_range(street)).map(<[_]>::to_vec),
        actions: Vec::new(),
    }
}

fn push_action(
    round: &mut RoundObj,
    player_id: u64,
    action: OhhAction,
    amount: u32,
    is_allin: bool,
) -> &mut ActionObj {
    round.actions.push(ActionObj {
        action_number: round.actions.len() as u64,
        player_id,
        action,
        amount: amount as f32,
        is_allin,
        cards: None,
    });
    round.actions.last_mut().expect("just pushed")
}

/// Reads an Open Hand History hand back as a [`Hand`]. The blind level is
/// unknown (OHH has no field for it), and chip amounts must be whole.
pub fn from_ohh(ohh: &HandHistory) -> Result<Hand, OhhError> {
    let fail = |reason: String| Err(OhhError(reason));
    let mut players = Vec::new();
    let mut index_of: HashMap<u64, usize> = HashMap::new();
    for p in &ohh.players {
        if p.is_sitting_out == Some(true) {
            continue;
        }
        index_of.insert(p.id, players.len());
        players.push(Player {
            seat: small(p.seat, "seat")?,
            name: p.name.clone(),
            stack: chips(p.starting_stack)?,
            cards: None,
            showed: false,
            collected: 0,
        });
    }
    let index = |id: u64| {
        index_of
            .get(&id)
            .copied()
            .ok_or_else(|| OhhError(format!("no player with id {id}")))
    };

    let mut rounds: Vec<&RoundObj> = ohh.rounds.iter().collect();
    rounds.sort_by_key(|r| r.id);
    let mut actions = Vec::new();
    let mut board = Vec::new();
    let mut put_in = vec![0u32; players.len()];
    for round in rounds {
        let street = match round.street.as_str() {
            "Preflop" => Some(Street::Preflop),
            "Flop" => Some(Street::Flop),
            "Turn" => Some(Street::Turn),
            "River" => Some(Street::River),
            "Showdown" => None,
            other => return fail(format!("unknown street {other:?}")),
        };
        board.extend(round.cards.iter().flatten().copied());
        let mut on_street = vec![0u32; players.len()];
        for action in &round.actions {
            let i = index(action.player_id)?;
            let added = chips(action.amount)?;
            let kind = match action.action {
                OhhAction::DealtCards | OhhAction::ShowsCards | OhhAction::MucksCards => {
                    let cards = action.cards.as_deref().unwrap_or_default();
                    let Ok(cards) = <[_; 2]>::try_from(cards) else {
                        return fail(format!("{} cards for a hold'em hand", cards.len()));
                    };
                    players[i].cards = Some(cards);
                    players[i].showed |= action.action == OhhAction::ShowsCards;
                    continue;
                }
                OhhAction::PostSmallBlind => ActionKind::SmallBlind(added),
                OhhAction::PostBigBlind => ActionKind::BigBlind(added),
                OhhAction::Fold => ActionKind::Fold,
                OhhAction::Check => ActionKind::Check,
                OhhAction::Call => ActionKind::Call(added),
                OhhAction::Bet => ActionKind::Bet(added),
                OhhAction::Raise => ActionKind::Raise {
                    to: on_street[i] + added,
                },
                ref other => return fail(format!("unsupported action {other:?}")),
            };
            let Some(street) = street else {
                return fail("a betting action in the showdown round".into());
            };
            on_street[i] += added;
            put_in[i] += added;
            actions.push(Action {
                street,
                seat: players[i].seat,
                kind,
                all_in: action.is_allin,
            });
        }
    }

    for pot in &ohh.pots {
        for win in &pot.player_wins {
            players[index(win.player_id)?].collected += chips(win.win_amount)?;
        }
    }
    let total: u32 = put_in.iter().sum();
    let collected: u32 = players.iter().map(|p| p.collected).sum();
    if collected != total {
        // The pots left out the part of the biggest bet nobody called.
        let mut sorted = put_in.clone();
        sorted.sort_unstable_by(|a, b| b.cmp(a));
        let uncalled = sorted[0] - sorted.get(1).copied().unwrap_or(0);
        if collected + uncalled != total {
            return fail(format!("{collected} chips won from {total} put in"));
        }
        let bettor = put_in.iter().position(|&p| p == sorted[0]).expect("max");
        players[bettor].collected += uncalled;
    }

    let tournament = ohh.tournament_info.as_ref().map(|t| {
        let cents = |amount: f32| Euros::from_cents((f64::from(amount) * 100.0).round() as u64);
        TournamentInfo {
            id: t.tournament_number.clone(),
            name: t.name.clone(),
            buy_in: BuyIn {
                prize: cents(t.buyin_amount),
                rake: cents(t.fee_amount),
            },
        }
    });
    let Some(started_at) = ohh.start_date_utc else {
        return fail("no start date".into());
    };
    Ok(Hand {
        game_number: ohh.game_number.clone(),
        started_at,
        tournament,
        level: None,
        table_name: ohh.table_name.clone(),
        table_size: small(ohh.table_size, "table size")?,
        button: small(ohh.dealer_seat, "dealer seat")?,
        small_blind: chips(ohh.small_blind_amount)?,
        big_blind: chips(ohh.big_blind_amount)?,
        hero: match ohh.hero_player_id {
            Some(id) => Some(players[index(id)?].seat),
            None => None,
        },
        players,
        actions,
        board,
    })
}

fn chips(amount: f32) -> Result<u32, OhhError> {
    if amount >= 0.0 && amount.fract() == 0.0 && amount <= u32::MAX as f32 {
        Ok(amount as u32)
    } else {
        Err(OhhError(format!("{amount} is not a whole number of chips")))
    }
}

fn small(value: u64, what: &str) -> Result<u8, OhhError> {
    u8::try_from(value).map_err(|_| OhhError(format!("{what} {value} out of range")))
}
