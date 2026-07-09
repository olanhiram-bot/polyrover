use serde::{Deserialize, Serialize};

use crate::{types::ClobOrderBook, Error, Result};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Request {
    pub token_id: String,
    pub side: String,
    pub amount: String,
    pub limit_price: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FillLevel {
    pub price: String,
    pub available_size: String,
    pub filled_size: String,
    pub notional: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct ResultRow {
    pub token_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub market: String,
    pub side: String,
    pub input_amount: String,
    pub input_amount_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub limit_price: String,
    pub complete: bool,
    pub filled_size: String,
    pub notional: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub average_price: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub expected_fill_price: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub best_price: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub worst_price: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub slippage: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub slippage_bps: String,
    pub unfilled_amount: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub book_hash: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub book_timestamp: String,
    pub levels: Vec<FillLevel>,
}

pub fn simulate_book(book: &ClobOrderBook, req: Request) -> Result<ResultRow> {
    let side = normalize_side(&req.side)?;
    let amount = parse_positive("--amount", &req.amount)?;
    let limit = if req.limit_price.trim().is_empty() {
        None
    } else {
        Some(parse_positive("--limit-price", &req.limit_price)?)
    };
    let mut levels = opposing_levels(book, side);
    levels.sort_by(|a, b| {
        if side == "buy" {
            a.0.total_cmp(&b.0)
        } else {
            b.0.total_cmp(&a.0)
        }
    });

    let best_price = levels.first().map(|l| fmt(l.0)).unwrap_or_default();
    let mut remaining = amount;
    let mut filled_size = 0.0;
    let mut notional = 0.0;
    let mut fills = Vec::new();
    let mut worst_price = String::new();

    for (price, size) in levels.iter().copied() {
        if limit
            .is_some_and(|max| (side == "buy" && price > max) || (side == "sell" && price < max))
        {
            break;
        }
        let (fill_size, fill_notional) = if side == "buy" {
            let level_notional = size * price;
            if remaining >= level_notional {
                (size, level_notional)
            } else {
                (remaining / price, remaining)
            }
        } else {
            let fill_size = remaining.min(size);
            (fill_size, fill_size * price)
        };
        if fill_size <= 0.0 {
            continue;
        }
        filled_size += fill_size;
        notional += fill_notional;
        fills.push(FillLevel {
            price: fmt(price),
            available_size: fmt(size),
            filled_size: fmt(fill_size),
            notional: fmt(fill_notional),
        });
        worst_price = fmt(price);
        remaining -= if side == "buy" {
            fill_notional
        } else {
            fill_size
        };
        if remaining <= 0.0 {
            remaining = 0.0;
            break;
        }
    }

    let mut out = ResultRow {
        token_id: if book.asset_id.is_empty() {
            req.token_id
        } else {
            book.asset_id.clone()
        },
        market: book.market.clone(),
        side: side.into(),
        input_amount: fmt(amount),
        input_amount_type: if side == "buy" { "usdc" } else { "shares" }.into(),
        limit_price: limit.map(fmt).unwrap_or_default(),
        complete: remaining == 0.0,
        filled_size: fmt(filled_size),
        notional: fmt(notional),
        best_price,
        worst_price,
        unfilled_amount: fmt(remaining),
        book_hash: book.hash.clone(),
        book_timestamp: book.timestamp.clone(),
        levels: fills,
        ..Default::default()
    };
    if filled_size > 0.0 {
        let avg = notional / filled_size;
        out.average_price = fmt(avg);
        out.expected_fill_price = out.average_price.clone();
        if let Ok(best) = out.best_price.parse::<f64>() {
            let slippage = if side == "buy" {
                avg - best
            } else {
                best - avg
            };
            out.slippage = fmt(slippage);
            out.slippage_bps = fmt(slippage / best * 10000.0);
        }
    }
    Ok(out)
}

fn opposing_levels(book: &ClobOrderBook, side: &str) -> Vec<(f64, f64)> {
    let rows = if side == "sell" {
        &book.bids
    } else {
        &book.asks
    };
    rows.iter()
        .filter_map(|level| {
            let price: f64 = level.price.trim().parse().ok()?;
            let size: f64 = level.size.trim().parse().ok()?;
            (price > 0.0 && size > 0.0 && price.is_finite() && size.is_finite())
                .then_some((price, size))
        })
        .collect()
}

fn normalize_side(side: &str) -> Result<&'static str> {
    match side.trim().to_ascii_lowercase().as_str() {
        "" | "buy" => Ok("buy"),
        "sell" => Ok("sell"),
        _ => Err(Error::Invalid("--side must be buy or sell".into())),
    }
}

fn parse_positive(name: &str, value: &str) -> Result<f64> {
    if value.contains('/') {
        return Err(Error::Invalid(format!("{name} must be a decimal")));
    }
    let n: f64 = value
        .trim()
        .parse()
        .map_err(|_| Error::Invalid(format!("{name} must be a positive decimal")))?;
    (n > 0.0 && n.is_finite())
        .then_some(n)
        .ok_or_else(|| Error::Invalid(format!("{name} must be a positive decimal")))
}

fn fmt(value: f64) -> String {
    let mut s = format!("{value:.6}");
    while s.contains('.') && s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ClobOrderBookLevel;

    #[test]
    fn buy_walks_asks_low_to_high() {
        let book = ClobOrderBook {
            asset_id: "tok".into(),
            asks: vec![
                ClobOrderBookLevel {
                    price: "0.6".into(),
                    size: "10".into(),
                },
                ClobOrderBookLevel {
                    price: "0.5".into(),
                    size: "4".into(),
                },
            ],
            ..Default::default()
        };
        let got = simulate_book(
            &book,
            Request {
                token_id: "tok".into(),
                side: "buy".into(),
                amount: "5".into(),
                limit_price: "".into(),
            },
        )
        .unwrap();
        assert!(got.complete);
        assert_eq!(got.filled_size, "9");
        assert_eq!(got.average_price, "0.555556");
        assert_eq!(got.best_price, "0.5");
    }

    #[test]
    fn sell_respects_limit_price() {
        let book = ClobOrderBook {
            bids: vec![
                ClobOrderBookLevel {
                    price: "0.4".into(),
                    size: "10".into(),
                },
                ClobOrderBookLevel {
                    price: "0.3".into(),
                    size: "10".into(),
                },
            ],
            ..Default::default()
        };
        let got = simulate_book(
            &book,
            Request {
                side: "sell".into(),
                amount: "12".into(),
                limit_price: "0.35".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!got.complete);
        assert_eq!(got.filled_size, "10");
        assert_eq!(got.unfilled_amount, "2");
    }
}
