use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::hand::{BuyIn, Euros};
use crate::parse::{
    ErrorKind, Line, NITRO, ParseError, err, lines, parse_buy_in, parse_euros, parse_number,
    parse_utc, unrecognised, unsupported,
};

pub(crate) const SUMMARY_HEADER: &str = "Winamax Poker - Tournament summary : ";

/// The metadata of one tournament, from its `_summary.txt` file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    pub tournament_id: String,
    pub tournament_name: String,
    /// The account the summary was written for.
    pub player: String,
    pub buy_in: BuyIn,
    pub registered_players: u32,
    /// The blind structure, level 1 first.
    pub levels: Vec<Level>,
    /// Money at stake (Expresso: the multiplier times the buy-in, before rake).
    pub prize_pool: Euros,
    pub started_at: DateTime<Utc>,
    /// How long the player stayed in the tournament.
    pub played: Duration,
    /// The player's finishing place, 1 for the winner.
    pub place: u32,
    pub winnings: Euros,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level {
    pub small_blind: u32,
    pub big_blind: u32,
    pub ante: u32,
    pub duration: Duration,
}

/// Parses the text of a tournament summary file.
pub fn parse_summary(text: &str) -> Result<Summary, ParseError> {
    let mut lines = lines(text);
    let Some(title) = lines.next() else {
        return Err(ParseError {
            line: 0,
            kind: ErrorKind::Malformed,
            reason: "empty summary".into(),
        });
    };
    let Some(name) = title.text.strip_prefix(SUMMARY_HEADER) else {
        return unrecognised(title);
    };
    // Other tournaments may add text after the id (` - Late Registration`).
    if !name.starts_with(&format!("{NITRO}(")) {
        return unsupported(title, format!("{name:?} is not an {NITRO}"));
    }
    let Some((tournament_name, tournament_id)) = name
        .strip_suffix(')')
        .and_then(|name| name.rsplit_once('('))
    else {
        return unrecognised(title);
    };

    let mut player = None;
    let mut buy_in = None;
    let mut registered_players = None;
    let mut levels = None;
    let mut prize_pool = None;
    let mut started_at = None;
    let mut played = None;
    let mut place = None;
    let mut winnings = Euros::default();
    let mut last = title;
    for line in lines {
        last = line;
        let text = line.text;
        if let Some((key, value)) = text.split_once(" : ") {
            match key {
                "Player" => player = Some(value.to_string()),
                "Buy-In" => buy_in = Some(parse_buy_in(line, value)?),
                "Registered players" => registered_players = Some(parse_number(line, value)?),
                // Since February 2023 Winamax writes `Levels : Levels : [...]`.
                "Levels" => {
                    let value = value.strip_prefix("Levels : ").unwrap_or(value);
                    levels = Some(parse_levels(line, value)?);
                }
                "Prizepool" => prize_pool = Some(parse_euros(line, value)?),
                "Mode" | "Type" | "Speed" | "Flight ID" => {}
                _ => return unrecognised(line),
            }
        } else if let Some(date) = text.strip_prefix("Tournament started ") {
            started_at = Some(parse_utc(line, date)?);
        } else if let Some(time) = text.strip_prefix("You played ") {
            played = Some(parse_played(line, time)?);
        } else if let Some(rest) = text.strip_prefix("You finished in ") {
            let Some(ordinal) = rest.strip_suffix(" place") else {
                return unrecognised(line);
            };
            let digits = ordinal.trim_end_matches(|c: char| c.is_ascii_alphabetic());
            place = Some(parse_number(line, digits)?);
        } else if let Some(amount) = text.strip_prefix("You won ") {
            winnings = parse_euros(line, amount)?;
        } else {
            return unrecognised(line);
        }
    }

    let missing = |field: &str| ParseError {
        line: last.number,
        kind: ErrorKind::Malformed,
        reason: format!("the summary has no {field}"),
    };
    Ok(Summary {
        tournament_id: tournament_id.to_string(),
        tournament_name: tournament_name.to_string(),
        player: player.ok_or_else(|| missing("player"))?,
        buy_in: buy_in.ok_or_else(|| missing("buy-in"))?,
        registered_players: registered_players.ok_or_else(|| missing("registered players"))?,
        levels: levels.ok_or_else(|| missing("levels"))?,
        prize_pool: prize_pool.ok_or_else(|| missing("prize pool"))?,
        started_at: started_at.ok_or_else(|| missing("start date"))?,
        played: played.ok_or_else(|| missing("time played"))?,
        place: place.ok_or_else(|| missing("finishing place"))?,
        winnings,
    })
}

/// `[10-20:0:60:holdem-no-limit,15-30:0:60:holdem-no-limit,…]`
fn parse_levels(line: Line, text: &str) -> Result<Vec<Level>, ParseError> {
    let Some(items) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) else {
        return unrecognised(line);
    };
    items
        .split(',')
        .map(|item| {
            let fields: Vec<&str> = item.split(':').collect();
            let [blinds, ante, seconds, _game] = fields[..] else {
                return err(line, format!("invalid level {item:?}"));
            };
            let Some((small_blind, big_blind)) = blinds.split_once('-') else {
                return err(line, format!("invalid level {item:?}"));
            };
            Ok(Level {
                small_blind: parse_number(line, small_blind)?,
                big_blind: parse_number(line, big_blind)?,
                ante: parse_number(line, ante)?,
                duration: Duration::from_secs(parse_number(line, seconds)?),
            })
        })
        .collect()
}

/// `5min 40s` or `48s`
fn parse_played(line: Line, text: &str) -> Result<Duration, ParseError> {
    let (minutes, seconds) = match text.split_once("min ") {
        Some((minutes, seconds)) => (parse_number(line, minutes)?, seconds),
        None => (0, text),
    };
    let Some(seconds) = seconds.strip_suffix('s') else {
        return err(line, format!("invalid duration {text:?}"));
    };
    let seconds: u64 = parse_number(line, seconds)?;
    Ok(Duration::from_secs(minutes * 60 + seconds))
}
