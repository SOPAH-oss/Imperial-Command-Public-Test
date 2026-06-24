use crate::models::{BlackjackResponse, CasinoBalance, RouletteResponse, SlotsResponse};
use anyhow::{anyhow, Result};
use rand::{seq::SliceRandom, Rng};
use std::{collections::HashMap, fs};

const BANK_FILE: &str = "bank.json";
const LEGACY_CASINO_FILE: &str = "casino.json";
const STARTING_CREDITS: i64 = 1_000;

#[derive(Debug, Clone)]
pub struct BlackjackGame {
    pub bet: i64,
    pub deck: Vec<u8>,
    pub player_cards: Vec<u8>,
    pub dealer_cards: Vec<u8>,
}

pub fn balance(username: &str) -> Result<CasinoBalance> {
    let username = clean_username(username);
    let mut balances = load_balances();
    let credits = *balances
        .entry(username.clone())
        .or_insert(STARTING_CREDITS);
    save_balances(&balances)?;
    Ok(CasinoBalance { username, credits })
}

pub fn set_balance(username: &str, credits: i64) -> Result<CasinoBalance> {
    let username = clean_username(username);
    if username.is_empty() {
        return Err(anyhow!("username is required"));
    }
    let mut balances = load_balances();
    balances.insert(username.clone(), credits);
    save_balances(&balances)?;
    Ok(CasinoBalance { username, credits })
}

pub fn all_balances() -> Result<Vec<CasinoBalance>> {
    let mut rows: Vec<_> = load_balances()
        .into_iter()
        .map(|(username, credits)| CasinoBalance { username, credits })
        .collect();
    rows.sort_by(|a, b| {
        b.credits
            .cmp(&a.credits)
            .then(a.username.cmp(&b.username))
    });
    Ok(rows)
}

pub fn play_roulette(username: &str, bet: i64, choice: &str) -> Result<RouletteResponse> {
    validate_bet(bet)?;
    charge_bet(username, bet)?;

    let mut rng = rand::thread_rng();
    let number = rng.gen_range(0..=36);
    let color = roulette_color(number).to_string();
    let choice = choice.trim().to_lowercase();
    let won = roulette_wins(number, &choice)?;
    let payout = if won {
        if choice == "green" || choice.parse::<u8>().is_ok() {
            bet * 36
        } else {
            bet * 2
        }
    } else {
        0
    };
    if payout > 0 {
        add_credits(username, payout)?;
    }
    let balance = balance(username)?;
    let mesadmin = if won {
        format!("Roulette hit {number} {color}. You won {payout} Credits.")
    } else {
        format!("Roulette hit {number} {color}. You lost {bet} Credits.")
    };
    Ok(RouletteResponse {
        number,
        color: color.into(),
        won,
        payout,
        mesadmin,
        balance,
    })
}

pub fn play_slots(username: &str, bet: i64) -> Result<SlotsResponse> {
    validate_bet(bet)?;
    charge_bet(username, bet)?;

    let symbols = ["Crown", "Pearl", "Admin", "Sword", "Shield", "Gold"];
    let mut rng = rand::thread_rng();
    let reels: Vec<String> = (0..3)
        .map(|_| symbols[rng.gen_range(0..symbols.len())].to_string())
        .collect();
    let payout = if reels[0] == reels[1] && reels[1] == reels[2] {
        if reels[0] == "Crown" {
            bet * 25
        } else {
            bet * 10
        }
    } else if reels[0] == reels[1] || reels[0] == reels[2] || reels[1] == reels[2] {
        bet * 2
    } else {
        0
    };
    if payout > 0 {
        add_credits(username, payout)?;
    }
    let balance = balance(username)?;
    let won = payout > 0;
    let mesadmin = if won {
        format!("Slots: {}. You won {payout} Credits.", reels.join(" | "))
    } else {
        format!("Slots: {}. You lost {bet} Credits.", reels.join(" | "))
    };
    Ok(SlotsResponse {
        reels,
        won,
        payout,
        mesadmin,
        balance,
    })
}

pub fn blackjack_start(username: &str, bet: i64) -> Result<(BlackjackGame, BlackjackResponse)> {
    validate_bet(bet)?;
    charge_bet(username, bet)?;

    let mut deck = shuffled_deck();
    let mut game = BlackjackGame {
        bet,
        deck: vec![],
        player_cards: vec![],
        dealer_cards: vec![],
    };
    std::mem::swap(&mut game.deck, &mut deck);
    draw_to(&mut game.deck, &mut game.player_cards)?;
    draw_to(&mut game.deck, &mut game.dealer_cards)?;
    draw_to(&mut game.deck, &mut game.player_cards)?;
    draw_to(&mut game.deck, &mut game.dealer_cards)?;

    if hand_total(&game.player_cards) == 21 {
        let payout = bet * 3;
        add_credits(username, payout)?;
        let response = blackjack_response(
            username,
            &game,
            true,
            "blackjack",
            format!("Blackjack. You won {payout} Credits."),
        )?;
        return Ok((game, response));
    }

    let response = blackjack_response(
        username,
        &game,
        false,
        "playing",
        "Blackjack started. Hit or stand.".to_string(),
    )?;
    Ok((game, response))
}

pub fn blackjack_hit(username: &str, game: &mut BlackjackGame) -> Result<BlackjackResponse> {
    draw_to(&mut game.deck, &mut game.player_cards)?;
    let total = hand_total(&game.player_cards);
    if total > 21 {
        return blackjack_response(
            username,
            game,
            true,
            "lost",
            format!("You busted with {total}."),
        );
    }
    blackjack_response(
        username,
        game,
        false,
        "playing",
        "Card drawn. Hit or stand.".to_string(),
    )
}

pub fn blackjack_stand(username: &str, game: &mut BlackjackGame) -> Result<BlackjackResponse> {
    while hand_total(&game.dealer_cards) < 17 {
        draw_to(&mut game.deck, &mut game.dealer_cards)?;
    }
    settle_blackjack(username, game)
}

pub fn play_blackjack_quick(username: &str, bet: i64) -> Result<BlackjackResponse> {
    let (mut game, response) = blackjack_start(username, bet)?;
    if response.finished {
        return Ok(response);
    }
    blackjack_stand(username, &mut game)
}

pub fn casino_help() -> &'static str {
    "Casino commands: !credits, !slots <bet>, !roulette <bet> <red|black|green|odd|even|0-36>, !blackjack <bet>. Currency: Credits."
}

fn settle_blackjack(username: &str, game: &BlackjackGame) -> Result<BlackjackResponse> {
    let player = hand_total(&game.player_cards);
    let dealer = hand_total(&game.dealer_cards);
    let (outcome, payout, mesadmin) = if dealer > 21 || player > dealer {
        (
            "won",
            game.bet * 2,
            format!("You beat the dealer {player} to {dealer}."),
        )
    } else if player == dealer {
        (
            "push",
            game.bet,
            format!("Push. You and the dealer both have {player}."),
        )
    } else {
        ("lost", 0, format!("Dealer wins {dealer} to {player}."))
    };
    if payout > 0 {
        add_credits(username, payout)?;
    }
    blackjack_response(username, game, true, outcome, mesadmin)
}

fn blackjack_response(
    username: &str,
    game: &BlackjackGame,
    finished: bool,
    outcome: &str,
    mesadmin: String,
) -> Result<BlackjackResponse> {
    let dealer_cards = if finished {
        game.dealer_cards.iter().map(|c| card_name(*c)).collect()
    } else {
        vec![card_name(game.dealer_cards[0]), "Hidden".to_string()]
    };
    Ok(BlackjackResponse {
        player_cards: game.player_cards.iter().map(|c| card_name(*c)).collect(),
        dealer_cards,
        player_total: hand_total(&game.player_cards),
        dealer_total: if finished {
            Some(hand_total(&game.dealer_cards))
        } else {
            None
        },
        finished,
        outcome: outcome.to_string(),
        mesadmin,
        balance: balance(username)?,
    })
}

fn load_balances() -> HashMap<String, i64> {
    fs::read_to_string(BANK_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .or_else(|| {
            fs::read_to_string(LEGACY_CASINO_FILE)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        })
        .unwrap_or_default()
}

fn save_balances(balances: &HashMap<String, i64>) -> Result<()> {
    fs::write(BANK_FILE, serde_json::to_string_pretty(balances)?)?;
    Ok(())
}

#[allow(dead_code)]
fn load_legacy_balances_only() -> HashMap<String, i64> {
    fs::read_to_string(LEGACY_CASINO_FILE)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn charge_bet(username: &str, bet: i64) -> Result<()> {
    let username = clean_username(username);
    let mut balances = load_balances();
    let entry = balances.entry(username).or_insert(STARTING_CREDITS);
    if *entry < bet {
        return Err(anyhow!("not enough Credits"));
    }
    *entry -= bet;
    save_balances(&balances)
}

pub fn add_credits(username: &str, amount: i64) -> Result<()> {
    let username = clean_username(username);
    let mut balances = load_balances();
    *balances.entry(username).or_insert(STARTING_CREDITS) += amount;
    save_balances(&balances)
}

fn validate_bet(bet: i64) -> Result<()> {
    if bet <= 0 {
        return Err(anyhow!("bet must be at least 1 Credits"));
    }
    if bet > 100_000 {
        return Err(anyhow!("bet is too large"));
    }
    Ok(())
}

fn clean_username(username: &str) -> String {
    username.trim().to_lowercase()
}

fn roulette_color(number: u8) -> &'static str {
    if number == 0 {
        "green"
    } else if matches!(
        number,
        1 | 3 | 5 | 7 | 9 | 12 | 14 | 16 | 18 | 19 | 21 | 23 | 25 | 27 | 30 | 32 | 34 | 36
    ) {
        "red"
    } else {
        "black"
    }
}

fn roulette_wins(number: u8, choice: &str) -> Result<bool> {
    Ok(match choice {
        "red" | "black" | "green" => roulette_color(number) == choice,
        "odd" => number != 0 && number % 2 == 1,
        "even" => number != 0 && number % 2 == 0,
        _ => {
            let picked = choice
                .parse::<u8>()
                .map_err(|_| anyhow!("invalid roulette choice"))?;
            if picked > 36 {
                return Err(anyhow!("roulette number must be 0-36"));
            }
            number == picked
        }
    })
}

fn shuffled_deck() -> Vec<u8> {
    let mut deck: Vec<u8> = (0..4).flat_map(|_| 1..=13).collect();
    deck.shuffle(&mut rand::thread_rng());
    deck
}

fn draw_to(deck: &mut Vec<u8>, hand: &mut Vec<u8>) -> Result<()> {
    let card = deck.pop().ok_or_else(|| anyhow!("deck is empty"))?;
    hand.push(card);
    Ok(())
}

fn card_name(card: u8) -> String {
    match card {
        1 => "A".to_string(),
        11 => "J".to_string(),
        12 => "Q".to_string(),
        13 => "K".to_string(),
        n => n.to_string(),
    }
}

fn hand_total(cards: &[u8]) -> u8 {
    let mut total = 0;
    let mut aces = 0;
    for card in cards {
        if *card == 1 {
            aces += 1;
            total += 11;
        } else {
            total += (*card).min(10);
        }
    }
    while total > 21 && aces > 0 {
        total -= 10;
        aces -= 1;
    }
    total
}
