use std::fmt;

use chrono::{DateTime, NaiveDateTime, Utc};
use rs_poker::core::Card;

use crate::hand::{Action, ActionKind, BuyIn, Euros, Hand, Player, Street, TournamentInfo};

/// How every Winamax hand, and every summary file, starts.
pub(crate) const WINAMAX_HEADER: &str = "Winamax Poker - ";

/// The only tournament format the parser reads.
pub(crate) const NITRO: &str = "Expresso Nitro";

/// Why a hand or a file could not be read, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number in the text, or 0 when the error concerns the
    /// whole file.
    pub line: usize,
    pub kind: ErrorKind,
    pub reason: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// The text breaks the Winamax grammar or does not add up.
    Malformed,
    /// A well-formed Winamax file of another format (cash game, another
    /// tournament), deliberately not read.
    Unsupported,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            ErrorKind::Malformed => "malformed",
            ErrorKind::Unsupported => "unsupported",
        };
        write!(f, "line {}: {kind}: {}", self.line, self.reason)
    }
}

impl std::error::Error for ParseError {}

/// Parses every hand of a hand-history file, in order. A malformed hand gives
/// an error in its place without affecting the others.
pub fn parse_hands(text: &str) -> Vec<Result<Hand, ParseError>> {
    split_hands(text)
        .into_iter()
        .map(|lines| HandParser::new(&lines).parse())
        .collect()
}

#[derive(Clone, Copy)]
pub(crate) struct Line<'a> {
    pub(crate) number: usize,
    pub(crate) text: &'a str,
}

/// The non-empty lines of `text`, numbered from 1, without trailing spaces
/// (Winamax pads some lines with one, e.g. `*** PRE-FLOP *** `).
pub(crate) fn lines(text: &str) -> impl Iterator<Item = Line<'_>> {
    text.lines()
        .enumerate()
        .map(|(index, raw)| Line {
            number: index + 1,
            text: raw.trim_end(),
        })
        .filter(|line| !line.text.is_empty())
}

/// Groups the lines by hand: a hand starts at each header line. Winamax
/// separates hands with blank lines, but splitting on headers keeps one
/// damaged separator from merging two hands.
fn split_hands(text: &str) -> Vec<Vec<Line<'_>>> {
    let mut hands: Vec<Vec<Line>> = Vec::new();
    for line in lines(text) {
        match hands.last_mut() {
            Some(hand) if !line.text.starts_with(WINAMAX_HEADER) => hand.push(line),
            _ => hands.push(vec![line]),
        }
    }
    hands
}

pub(crate) fn err<T>(line: Line, reason: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError {
        line: line.number,
        kind: ErrorKind::Malformed,
        reason: reason.into(),
    })
}

pub(crate) fn unsupported<T>(line: Line, reason: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError {
        line: line.number,
        kind: ErrorKind::Unsupported,
        reason: reason.into(),
    })
}

pub(crate) fn unrecognised<T>(line: Line) -> Result<T, ParseError> {
    err(line, format!("unrecognised line: {:?}", line.text))
}

/// Chips a player has put in, and still has, during the hand.
#[derive(Clone, Copy, Default)]
struct Chips {
    remaining: u32,
    on_street: u32,
    total: u32,
}

struct HandParser<'a> {
    lines: &'a [Line<'a>],
    next: usize,
    players: Vec<Player>,
    chips: Vec<Chips>,
    actions: Vec<Action>,
    board: Vec<Card>,
    street: Street,
    big_blind: u32,
}

impl<'a> HandParser<'a> {
    fn new(lines: &'a [Line<'a>]) -> Self {
        HandParser {
            lines,
            next: 0,
            players: Vec::new(),
            chips: Vec::new(),
            actions: Vec::new(),
            board: Vec::new(),
            street: Street::Preflop,
            big_blind: 0,
        }
    }

    fn peek(&self) -> Option<Line<'a>> {
        self.lines.get(self.next).copied()
    }

    fn take(&mut self) -> Result<Line<'a>, ParseError> {
        match self.peek() {
            Some(line) => {
                self.next += 1;
                Ok(line)
            }
            None => {
                let last = *self.lines.last().expect("a hand has a header line");
                err(last, "the hand ends before its summary")
            }
        }
    }

    fn expect(&mut self, text: &str) -> Result<(), ParseError> {
        let line = self.take()?;
        if line.text == text {
            Ok(())
        } else {
            err(line, format!("expected {text:?}, found {:?}", line.text))
        }
    }

    fn parse(mut self) -> Result<Hand, ParseError> {
        let header = self.take()?;
        let header_fields = parse_header(header)?;
        self.big_blind = header_fields.big_blind;
        let table = self.take()?;
        let (table_name, table_size, button) = parse_table(table)?;
        let tournament_id = tournament_id_of_table(&table_name)
            .ok_or(())
            .or_else(|()| err(table, "no tournament id in the table name"))?;

        while let Some(line) = self.peek() {
            if !line.text.starts_with("Seat ") {
                break;
            }
            self.next += 1;
            let player = parse_seat(line)?;
            self.chips.push(Chips {
                remaining: player.stack,
                ..Chips::default()
            });
            self.players.push(player);
        }
        self.expect("*** ANTE/BLINDS ***")?;

        let mut hero = None;
        loop {
            let line = self.take()?;
            if line.text == "*** PRE-FLOP ***" {
                break;
            }
            if let Some(rest) = line.text.strip_prefix("Dealt to ") {
                let (index, cards) = self.player_prefix(line, rest)?;
                self.players[index].cards = Some(parse_hole_cards(line, cards)?);
                hero = Some(self.players[index].seat);
                continue;
            }
            let (index, rest) = self.player_prefix(line, line.text)?;
            let (rest, all_in) = strip_all_in(rest);
            let kind = if let Some(amount) = rest.strip_prefix("posts small blind ") {
                ActionKind::SmallBlind(parse_chips(line, amount)?)
            } else if let Some(amount) = rest.strip_prefix("posts big blind ") {
                ActionKind::BigBlind(parse_chips(line, amount)?)
            } else {
                return unrecognised(line);
            };
            self.record(line, index, kind, all_in)?;
        }

        loop {
            let line = self.take()?;
            match line.text {
                "*** SUMMARY ***" => break,
                "*** SHOW DOWN ***" => continue,
                _ => {}
            }
            if let Some(rest) = line.text.strip_prefix("*** ") {
                self.deal(line, rest)?;
                continue;
            }
            let (index, rest) = self.player_prefix(line, line.text)?;
            if let Some(cards) = rest.strip_prefix("shows ") {
                let cards = cards.split_once(" (").map_or(cards, |(cards, _)| cards);
                self.players[index].cards = Some(parse_hole_cards(line, cards)?);
                self.players[index].showed = true;
            } else if let Some(rest) = rest.strip_prefix("collected ") {
                let Some((amount, _pot)) = rest.split_once(" from ") else {
                    return unrecognised(line);
                };
                self.players[index].collected += parse_chips(line, amount)?;
            } else {
                let (rest, all_in) = strip_all_in(rest);
                let kind = self.parse_action(line, rest)?;
                self.record(line, index, kind, all_in)?;
            }
        }
        self.parse_summary()?;

        Ok(Hand {
            game_number: header_fields.game_number,
            started_at: header_fields.started_at,
            tournament: Some(TournamentInfo {
                id: tournament_id,
                name: header_fields.tournament_name,
                buy_in: header_fields.buy_in,
            }),
            level: Some(header_fields.level),
            table_name,
            table_size,
            button,
            small_blind: header_fields.small_blind,
            big_blind: header_fields.big_blind,
            players: self.players,
            hero,
            actions: self.actions,
            board: self.board,
        })
    }

    /// Finds the player whose name starts `text`, followed by a space, and
    /// returns their index and the rest of the text. Names may contain
    /// spaces, so the longest matching name wins.
    fn player_prefix<'t>(&self, line: Line, text: &'t str) -> Result<(usize, &'t str), ParseError> {
        self.players
            .iter()
            .enumerate()
            .filter_map(|(index, player)| {
                let rest = text.strip_prefix(player.name.as_str())?.strip_prefix(' ')?;
                Some((player.name.len(), index, rest))
            })
            .max_by_key(|&(len, _, _)| len)
            .map(|(_, index, rest)| (index, rest))
            .ok_or(())
            .or_else(|()| unrecognised(line))
    }

    /// Checks the `*** SUMMARY ***` section against the hand: total pot, board,
    /// and what each listed player showed and won.
    fn parse_summary(&mut self) -> Result<(), ParseError> {
        let line = self.take()?;
        let Some(total) = line
            .text
            .strip_prefix("Total pot ")
            .and_then(|rest| rest.strip_suffix(" | No rake"))
        else {
            return unrecognised(line);
        };
        let total = parse_chips(line, total)?;
        let put_in: u32 = self.chips.iter().map(|c| c.total).sum();
        if total != put_in {
            return err(line, format!("total pot {total} but {put_in} chips put in"));
        }
        let collected: u32 = self.players.iter().map(|p| p.collected).sum();
        if collected != total {
            return err(
                line,
                format!("{collected} chips collected from a total pot of {total}"),
            );
        }
        while let Some(line) = self.peek() {
            self.next += 1;
            if let Some(board) = line.text.strip_prefix("Board: ") {
                if parse_cards(line, board)? != self.board {
                    return err(line, "the summary board differs from the streets");
                }
            } else {
                self.check_seat_summary(line)?;
            }
        }
        Ok(())
    }

    /// `Seat 2: hero (button) showed [Js Jh] and won 180 with One pair : Jacks`,
    /// `… showed [6s Qd] and lost with …` or `Seat 1: villain 1 (big blind) won 30`.
    fn check_seat_summary(&self, line: Line) -> Result<(), ParseError> {
        let Some((seat, rest)) = line
            .text
            .strip_prefix("Seat ")
            .and_then(|rest| rest.split_once(": "))
        else {
            return unrecognised(line);
        };
        let seat: u8 = parse_number(line, seat)?;
        let Some(player) = self.players.iter().find(|p| p.seat == seat) else {
            return err(line, format!("no player on seat {seat}"));
        };
        let Some(mut rest) = rest.strip_prefix(player.name.as_str()) else {
            return err(line, format!("seat {seat} is not {:?}", player.name));
        };
        while let Some(after) = [" (small blind)", " (big blind)", " (button)"]
            .iter()
            .find_map(|tag| rest.strip_prefix(tag))
        {
            rest = after;
        }
        let (shown, won) = if let Some(won) = rest.strip_prefix(" won ") {
            (None, parse_chips(line, won)?)
        } else if let Some((cards, outcome)) = rest
            .strip_prefix(" showed ")
            .and_then(|rest| rest.split_once(" and "))
        {
            let won = match outcome.strip_prefix("won ") {
                Some(won) => {
                    let amount = won.split_once(" with ").map_or(won, |(amount, _)| amount);
                    parse_chips(line, amount)?
                }
                None if outcome.starts_with("lost") => 0,
                None => return unrecognised(line),
            };
            (Some(parse_hole_cards(line, cards)?), won)
        } else {
            return unrecognised(line);
        };
        if won != player.collected {
            return err(
                line,
                format!("won {won} but collected {}", player.collected),
            );
        }
        if shown.is_some() && (!player.showed || shown != player.cards) {
            return err(line, "showed cards that were not shown during the hand");
        }
        Ok(())
    }

    /// Handles a street header such as `FLOP *** [6h 3h 5d]` (the leading
    /// `*** ` already removed).
    fn deal(&mut self, line: Line, rest: &str) -> Result<(), ParseError> {
        let (street, cards) = if let Some(cards) = rest.strip_prefix("FLOP *** ") {
            (Street::Flop, cards)
        } else if let Some(cards) = rest.strip_prefix("TURN *** ") {
            (Street::Turn, cards)
        } else if let Some(cards) = rest.strip_prefix("RIVER *** ") {
            (Street::River, cards)
        } else {
            return unrecognised(line);
        };
        let new_cards = match cards.split_once("][") {
            // Turn and river repeat the board so far, then the new card.
            Some((old, new)) => {
                let old = parse_cards(line, &format!("{old}]"))?;
                if old != self.board {
                    return err(line, "the board does not match the previous streets");
                }
                parse_cards(line, &format!("[{new}"))?
            }
            None => parse_cards(line, cards)?,
        };
        let expected = match street {
            Street::Flop => 3,
            _ => 1,
        };
        if new_cards.len() != expected || street as usize != self.street as usize + 1 {
            return err(line, "streets out of order");
        }
        self.board.extend(new_cards);
        self.street = street;
        for chips in &mut self.chips {
            chips.on_street = 0;
        }
        Ok(())
    }

    /// The street total a player must reach to stay in. Preflop it is never
    /// below the big blind, even when the big blind was posted short (all-in).
    fn amount_to_match(&self) -> u32 {
        let highest = self.highest_on_street();
        match self.street {
            Street::Preflop => highest.max(self.big_blind),
            _ => highest,
        }
    }

    fn highest_on_street(&self) -> u32 {
        self.chips.iter().map(|c| c.on_street).max().unwrap_or(0)
    }

    /// A betting action, without the player name and the all-in suffix.
    fn parse_action(&self, line: Line, text: &str) -> Result<ActionKind, ParseError> {
        Ok(match text {
            "folds" => ActionKind::Fold,
            "checks" => ActionKind::Check,
            _ => {
                if let Some(amount) = text.strip_prefix("calls ") {
                    ActionKind::Call(parse_chips(line, amount)?)
                } else if let Some(amount) = text.strip_prefix("bets ") {
                    ActionKind::Bet(parse_chips(line, amount)?)
                } else if let Some(rest) = text.strip_prefix("raises ") {
                    let Some((by, to)) = rest.split_once(" to ") else {
                        return unrecognised(line);
                    };
                    let (by, to) = (parse_chips(line, by)?, parse_chips(line, to)?);
                    // Winamax writes `raises X to Y` with X = Y − the highest
                    // street total so far, even below a short (all-in) big
                    // blind, where calling still costs the full big blind.
                    let highest = self.highest_on_street();
                    if to <= highest || by != to - highest {
                        return err(line, format!("raise to {to} by {by} over {highest}"));
                    }
                    ActionKind::Raise { to }
                } else {
                    return unrecognised(line);
                }
            }
        })
    }

    /// Applies an action to the player's chips, checking that it is
    /// consistent with the amount to match and the player's stack.
    fn record(
        &mut self,
        line: Line,
        index: usize,
        kind: ActionKind,
        all_in: bool,
    ) -> Result<(), ParseError> {
        let to_match = self.amount_to_match();
        let street = self.street;
        let chips = &mut self.chips[index];
        let added = match kind {
            ActionKind::SmallBlind(n)
            | ActionKind::BigBlind(n)
            | ActionKind::Call(n)
            | ActionKind::Bet(n) => n,
            ActionKind::Raise { to } => to - chips.on_street,
            ActionKind::Fold | ActionKind::Check => 0,
        };
        let consistent = match kind {
            ActionKind::Check => chips.on_street == to_match,
            // Only an all-in call may fall short of the amount to match.
            ActionKind::Call(n) if all_in => n > 0 && chips.on_street + n <= to_match,
            ActionKind::Call(n) => n > 0 && chips.on_street + n == to_match,
            ActionKind::Bet(n) => n > 0 && street != Street::Preflop && to_match == 0,
            _ => true,
        };
        if !consistent {
            return err(
                line,
                format!(
                    "{kind:?} does not fit a street total of {} facing {to_match}",
                    chips.on_street
                ),
            );
        }
        if added > chips.remaining {
            return err(
                line,
                format!("puts in {added} with {} left", chips.remaining),
            );
        }
        chips.remaining -= added;
        if all_in != (chips.remaining == 0 && added > 0) {
            return err(
                line,
                format!("all-in flag wrong with {} left", chips.remaining),
            );
        }
        chips.on_street += added;
        chips.total += added;
        self.actions.push(Action {
            street: self.street,
            seat: self.players[index].seat,
            kind,
            all_in,
        });
        Ok(())
    }
}

struct Header {
    tournament_name: String,
    buy_in: BuyIn,
    level: u32,
    game_number: String,
    small_blind: u32,
    big_blind: u32,
    started_at: DateTime<Utc>,
}

/// `Winamax Poker - Tournament "Expresso Nitro" buyIn: 0.93€ + 0.07€ level: 1
/// - HandId: #…-4-1672660021 - Holdem no limit (10/20) - 2023/01/02 11:47:01 UTC`
fn parse_header(line: Line) -> Result<Header, ParseError> {
    let Some(rest) = line.text.strip_prefix(WINAMAX_HEADER) else {
        return unrecognised(line);
    };
    match rest
        .strip_prefix("Tournament \"")
        .and_then(|rest| rest.split_once('"'))
    {
        Some((NITRO, _)) => {}
        Some((name, _)) => return unsupported(line, format!("{name:?} is not an {NITRO}")),
        None => return unsupported(line, "not a tournament hand"),
    }
    let fields = (|| {
        let rest = line.text.strip_prefix("Winamax Poker - Tournament \"")?;
        let (name, rest) = rest.split_once("\" buyIn: ")?;
        let (buy_in, rest) = rest.split_once(" level: ")?;
        let (level, rest) = rest.split_once(" - HandId: #")?;
        let (game_number, rest) = rest.split_once(" - Holdem no limit (")?;
        let (blinds, date) = rest.split_once(") - ")?;
        Some((name, buy_in, level, game_number, blinds, date))
    })();
    let Some((name, buy_in, level, game_number, blinds, date)) = fields else {
        return unrecognised(line);
    };
    let Some((small_blind, big_blind)) = blinds.split_once('/') else {
        return unrecognised(line);
    };
    Ok(Header {
        tournament_name: name.to_string(),
        buy_in: parse_buy_in(line, buy_in)?,
        level: parse_number(line, level)?,
        game_number: game_number.to_string(),
        small_blind: parse_chips(line, small_blind)?,
        big_blind: parse_chips(line, big_blind)?,
        started_at: parse_utc(line, date)?,
    })
}

/// `Table: 'Expresso Nitro(618031930)#0' 3-max (real money) Seat #2 is the button`
fn parse_table(line: Line) -> Result<(String, u8, u8), ParseError> {
    let fields = (|| {
        let rest = line.text.strip_prefix("Table: '")?;
        let (name, rest) = rest.rsplit_once("' ")?;
        let (size, rest) = rest.split_once("-max (real money) Seat #")?;
        let button = rest.strip_suffix(" is the button")?;
        Some((name, size, button))
    })();
    let Some((name, size, button)) = fields else {
        return unrecognised(line);
    };
    Ok((
        name.to_string(),
        parse_number(line, size)?,
        parse_number(line, button)?,
    ))
}

/// The id between the last parentheses of `Expresso Nitro(618031930)#0`.
fn tournament_id_of_table(table_name: &str) -> Option<String> {
    let (_, rest) = table_name.rsplit_once('(')?;
    let (id, _) = rest.split_once(')')?;
    Some(id.to_string())
}

/// `Seat 1: villain 1 (270)`
fn parse_seat(line: Line) -> Result<Player, ParseError> {
    let fields = (|| {
        let rest = line.text.strip_prefix("Seat ")?;
        let (seat, rest) = rest.split_once(": ")?;
        let (name, stack) = rest.rsplit_once(" (")?;
        Some((seat, name, stack.strip_suffix(')')?))
    })();
    let Some((seat, name, stack)) = fields else {
        return unrecognised(line);
    };
    Ok(Player {
        seat: parse_number(line, seat)?,
        name: name.to_string(),
        stack: parse_chips(line, stack)?,
        cards: None,
        showed: false,
        collected: 0,
    })
}

fn strip_all_in(text: &str) -> (&str, bool) {
    match text.strip_suffix(" and is all-in") {
        Some(text) => (text, true),
        None => (text, false),
    }
}

pub(crate) fn parse_number<T: std::str::FromStr>(line: Line, text: &str) -> Result<T, ParseError> {
    // `FromStr` for integers accepts a leading `+`; Winamax never writes one.
    if !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && let Ok(value) = text.parse()
    {
        return Ok(value);
    }
    err(line, format!("invalid number {text:?}"))
}

fn parse_chips(line: Line, text: &str) -> Result<u32, ParseError> {
    parse_number(line, text)
}

/// `0.93€ + 0.07€`
pub(crate) fn parse_buy_in(line: Line, text: &str) -> Result<BuyIn, ParseError> {
    let Some((prize, rake)) = text.split_once(" + ") else {
        return err(line, format!("invalid buy-in {text:?}"));
    };
    Ok(BuyIn {
        prize: parse_euros(line, prize)?,
        rake: parse_euros(line, rake)?,
    })
}

/// `0.93€`, `2€`
pub(crate) fn parse_euros(line: Line, text: &str) -> Result<Euros, ParseError> {
    let (whole, cents) = match text
        .strip_suffix('€')
        .map(|a| a.split_once('.').unwrap_or((a, "00")))
    {
        Some((whole, cents)) if cents.len() == 2 => (whole, cents),
        _ => return err(line, format!("invalid amount {text:?}")),
    };
    let whole: u64 = parse_number(line, whole)?;
    let cents: u64 = parse_number(line, cents)?;
    Ok(Euros::from_cents(whole * 100 + cents))
}

/// `2023/01/02 11:47:01 UTC`
pub(crate) fn parse_utc(line: Line, text: &str) -> Result<DateTime<Utc>, ParseError> {
    text.strip_suffix(" UTC")
        .and_then(|text| NaiveDateTime::parse_from_str(text, "%Y/%m/%d %H:%M:%S").ok())
        .map(|date| date.and_utc())
        .ok_or(())
        .or_else(|()| err(line, format!("invalid date {text:?}")))
}

/// `[6h 3h 5d]`
fn parse_cards(line: Line, text: &str) -> Result<Vec<Card>, ParseError> {
    let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) else {
        return err(line, format!("invalid cards {text:?}"));
    };
    inner
        .split(' ')
        .map(|card| match Card::try_from(card) {
            // `Card::try_from` ignores anything after the first two characters.
            Ok(parsed) if card.len() == 2 => Ok(parsed),
            _ => err(line, format!("invalid card {card:?}")),
        })
        .collect()
}

fn parse_hole_cards(line: Line, text: &str) -> Result<[Card; 2], ParseError> {
    parse_cards(line, text)?
        .try_into()
        .or_else(|_| err(line, "expected two hole cards"))
}
