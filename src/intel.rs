use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const FORMULA_WALLET_SCORE_V1: &str = "wallet_score_v1";
pub const CONFIDENCE_NONE: &str = "none";
pub const CONFIDENCE_LOW: &str = "low";
pub const CONFIDENCE_MEDIUM: &str = "medium";
pub const CONFIDENCE_HIGH: &str = "high";
const DEFAULT_PRIOR_WINS: f64 = 10.0;
const DEFAULT_PRIOR_BETS: f64 = 20.0;
const CANDIDATE_LANGUAGE: &str = "statistical candidate signal; not a finding of misconduct";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WalletScoreMetrics {
    pub wins: i32,
    pub bets: i32,
    pub volume: f64,
    pub realized_pnl: f64,
    pub roi: f64,
    pub raw_win_rate: f64,
    pub shrinkage_win_rate: f64,
    pub category_edge: f64,
    pub concentration_signal: bool,
    pub late_entry_signal: bool,
    pub co_positioning_signal: bool,
    pub shrinkage_prior_wins: f64,
    pub shrinkage_prior_bets: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WalletScore {
    pub wallet: String,
    pub value: i32,
    pub confidence: String,
    pub formula_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_of: Option<DateTime<Utc>>,
    pub source_rows: i32,
    pub reasons: Vec<String>,
    pub raw_metrics: WalletScoreMetrics,
    pub language: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScoreInput {
    pub wallet: String,
    pub wins: i32,
    pub bets: i32,
    pub volume: f64,
    pub realized_pnl: f64,
    pub category_edge: f64,
    pub concentration_signal: bool,
    pub late_entry_signal: bool,
    pub co_positioning_signal: bool,
    pub prior_wins: f64,
    pub prior_bets: f64,
    pub as_of: Option<DateTime<Utc>>,
    pub source_rows: i32,
}

pub fn shrinkage_win_rate(wins: i32, bets: i32, prior_wins: f64, prior_bets: f64) -> f64 {
    let bets = bets.max(0);
    let wins = wins.max(0).min(bets);
    let prior_bets = prior_bets.max(0.0);
    let mut prior_wins = prior_wins.max(0.0);
    if prior_bets > 0.0 && prior_wins > prior_bets {
        prior_wins = prior_bets;
    }
    let denominator = bets as f64 + prior_bets;
    if denominator == 0.0 {
        0.0
    } else {
        (wins as f64 + prior_wins) / denominator
    }
}

pub fn roi(realized_pnl: f64, volume: f64) -> f64 {
    if volume <= 0.0 {
        0.0
    } else {
        realized_pnl / volume
    }
}

pub fn score_wallet(input: ScoreInput) -> WalletScore {
    let (prior_wins, prior_bets) = normalize_prior(input.prior_wins, input.prior_bets);
    let (wins, bets) = normalize_record(input.wins, input.bets);
    let raw_win_rate = if bets > 0 {
        wins as f64 / bets as f64
    } else {
        0.0
    };
    let roi = roi(input.realized_pnl, input.volume);
    let shrinkage = shrinkage_win_rate(wins, bets, prior_wins, prior_bets);
    let mut value = 0;
    let mut reasons = Vec::new();

    add_score(&mut value, &mut reasons, sample_score(bets));
    add_score(&mut value, &mut reasons, shrinkage_score(shrinkage));
    if input.realized_pnl > 0.0 {
        value += 15;
        reasons.push("positive realized PnL".into());
    }
    add_score(&mut value, &mut reasons, roi_score(roi));
    add_score(
        &mut value,
        &mut reasons,
        category_edge_score(input.category_edge),
    );
    if input.concentration_signal {
        value += 5;
        reasons.push("concentrated exposure requires review".into());
    }
    if input.late_entry_signal {
        value += 5;
        reasons.push("late market entry requires review".into());
    }
    if input.co_positioning_signal {
        value += 5;
        reasons.push("repeat co-positioning suggests potential coordination signal".into());
    }
    value = value.clamp(0, 100);

    WalletScore {
        wallet: input.wallet,
        value,
        confidence: confidence_for_bets(bets).into(),
        formula_version: FORMULA_WALLET_SCORE_V1.into(),
        as_of: input.as_of,
        source_rows: input.source_rows,
        reasons,
        language: CANDIDATE_LANGUAGE.into(),
        raw_metrics: WalletScoreMetrics {
            wins,
            bets,
            volume: input.volume,
            realized_pnl: input.realized_pnl,
            roi,
            raw_win_rate,
            shrinkage_win_rate: shrinkage,
            category_edge: input.category_edge,
            concentration_signal: input.concentration_signal,
            late_entry_signal: input.late_entry_signal,
            co_positioning_signal: input.co_positioning_signal,
            shrinkage_prior_wins: prior_wins,
            shrinkage_prior_bets: prior_bets,
        },
    }
}

fn add_score(value: &mut i32, reasons: &mut Vec<String>, item: (i32, &'static str)) {
    *value += item.0;
    if !item.1.is_empty() {
        reasons.push(item.1.into());
    }
}
fn normalize_prior(w: f64, b: f64) -> (f64, f64) {
    if w <= 0.0 && b <= 0.0 {
        return (DEFAULT_PRIOR_WINS, DEFAULT_PRIOR_BETS);
    }
    let mut w = w.max(0.0);
    let b = if b <= 0.0 { DEFAULT_PRIOR_BETS } else { b };
    if w > b {
        w = b;
    }
    (w, b)
}
fn normalize_record(wins: i32, bets: i32) -> (i32, i32) {
    let bets = bets.max(0);
    (wins.max(0).min(bets), bets)
}
fn sample_score(bets: i32) -> (i32, &'static str) {
    match bets {
        100.. => (20, "large enough sample for high-confidence interpretation"),
        30..=99 => (12, "moderate sample for interpretation"),
        1..=29 => (5, "small sample; score is heavily discounted"),
        _ => (0, ""),
    }
}
fn shrinkage_score(rate: f64) -> (i32, &'static str) {
    if rate >= 0.65 {
        (25, "shrinkage-adjusted win rate is materially above prior")
    } else if rate >= 0.58 {
        (18, "shrinkage-adjusted win rate is above prior")
    } else if rate >= 0.52 {
        (10, "shrinkage-adjusted win rate is slightly above prior")
    } else {
        (0, "")
    }
}
fn roi_score(rate: f64) -> (i32, &'static str) {
    if rate >= 0.10 {
        (15, "ROI is strongly positive")
    } else if rate >= 0.02 {
        (10, "ROI is positive")
    } else if rate > 0.0 {
        (5, "ROI is slightly positive")
    } else {
        (0, "")
    }
}
fn category_edge_score(edge: f64) -> (i32, &'static str) {
    if edge >= 0.08 {
        (10, "category edge is elevated")
    } else if edge >= 0.03 {
        (5, "category edge is positive")
    } else {
        (0, "")
    }
}
fn confidence_for_bets(bets: i32) -> &'static str {
    match bets {
        100.. => CONFIDENCE_HIGH,
        30..=99 => CONFIDENCE_MEDIUM,
        1..=29 => CONFIDENCE_LOW,
        _ => CONFIDENCE_NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinkage_and_roi_match_go_rules() {
        assert!((shrinkage_win_rate(2, 2, 10.0, 20.0) - 12.0 / 22.0).abs() < 1e-9);
        assert!((shrinkage_win_rate(5, 2, 10.0, 20.0) - 12.0 / 22.0).abs() < 1e-9);
        assert_eq!(roi(25.0, 0.0), 0.0);
        assert_eq!(roi(-5.0, 100.0), -0.05);
    }

    #[test]
    fn score_wallet_is_deterministic_and_safe_language() {
        let score = score_wallet(ScoreInput {
            wallet: "0xwallet".into(),
            wins: 95,
            bets: 150,
            volume: 10000.0,
            realized_pnl: 1200.0,
            category_edge: 0.09,
            concentration_signal: true,
            late_entry_signal: true,
            co_positioning_signal: true,
            source_rows: 300,
            ..Default::default()
        });
        assert_eq!(score.value, 93);
        assert_eq!(score.confidence, CONFIDENCE_HIGH);
        assert_eq!(score.raw_metrics.roi, 0.12);
        assert!(score
            .reasons
            .join(" | ")
            .contains("potential coordination signal"));
        assert!(score.language.contains("not a finding"));
    }

    #[test]
    fn discounts_empty_losing_and_small_records() {
        assert_eq!(score_wallet(ScoreInput::default()).value, 0);
        assert_eq!(
            score_wallet(ScoreInput {
                wins: 10,
                bets: 40,
                volume: 1000.0,
                realized_pnl: -100.0,
                ..Default::default()
            })
            .value,
            12
        );
        assert_eq!(
            score_wallet(ScoreInput {
                wins: 2,
                bets: 2,
                volume: 100.0,
                realized_pnl: 10.0,
                ..Default::default()
            })
            .value,
            45
        );
    }
}
