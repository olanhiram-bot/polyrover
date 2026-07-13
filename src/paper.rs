//! Local paper-trading state: orders, fills, and positions (no network).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::{Error, Result};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Order {
    pub market_id: String,
    pub token_id: String,
    pub price: f64,
    pub size: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Fill {
    pub market_id: String,
    pub token_id: String,
    pub price: f64,
    pub size: f64,
    pub live: bool,
    #[serde(skip_serializing_if = "is_zero")]
    pub realized_pnl: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub token_id: String,
    pub size: f64,
    pub cost: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct State {
    pub currency: String,
    pub cash: f64,
    pub positions: BTreeMap<String, Position>,
    pub fills: Vec<Fill>,
}

const SIZE_EPSILON: f64 = 1e-9;

impl State {
    pub fn new(currency: impl Into<String>, cash: f64) -> Self {
        Self {
            currency: currency.into(),
            cash,
            positions: BTreeMap::new(),
            fills: vec![],
        }
    }

    pub fn buy(&mut self, order: Order) -> Result<Fill> {
        let cost = order.price * order.size;
        if cost > self.cash {
            return Err(Error::Invalid("insufficient paper cash".into()));
        }
        self.cash -= cost;
        let pos = self
            .positions
            .entry(order.token_id.clone())
            .or_insert_with(|| Position {
                token_id: order.token_id.clone(),
                ..Default::default()
            });
        pos.size += order.size;
        pos.cost += cost;
        let fill = Fill {
            market_id: order.market_id,
            token_id: order.token_id,
            price: order.price,
            size: order.size,
            live: false,
            realized_pnl: 0.0,
        };
        self.fills.push(fill.clone());
        Ok(fill)
    }

    pub fn sell(&mut self, order: Order) -> Result<Fill> {
        if order.size <= 0.0 {
            return Err(Error::Invalid("sell size must be positive".into()));
        }
        let pos = self
            .positions
            .get(&order.token_id)
            .cloned()
            .unwrap_or_default();
        if pos.size <= 0.0 || order.size > pos.size + SIZE_EPSILON {
            return Err(Error::Invalid("insufficient paper position".into()));
        }
        let avg_cost = pos.cost / pos.size;
        let proceeds = order.price * order.size;
        let realized = proceeds - avg_cost * order.size;
        let remaining_size = pos.size - order.size;

        self.cash += proceeds;
        if remaining_size <= SIZE_EPSILON {
            self.positions.remove(&order.token_id);
        } else {
            self.positions.insert(
                order.token_id.clone(),
                Position {
                    token_id: order.token_id.clone(),
                    size: remaining_size,
                    cost: avg_cost * remaining_size,
                },
            );
        }
        let fill = Fill {
            market_id: order.market_id,
            token_id: order.token_id,
            price: order.price,
            size: order.size,
            live: false,
            realized_pnl: realized,
        };
        self.fills.push(fill.clone());
        Ok(fill)
    }
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buy_debits_cash_and_adds_position() {
        let mut state = State::new("USD", 100.0);
        state
            .buy(Order {
                market_id: "m".into(),
                token_id: "t".into(),
                price: 0.4,
                size: 10.0,
            })
            .unwrap();
        assert_eq!(state.cash, 96.0);
        assert_eq!(state.positions["t"].size, 10.0);
        assert_eq!(state.positions["t"].cost, 4.0);
    }

    #[test]
    fn sell_uses_average_cost_and_rejects_shorts() {
        let mut state = State::new("USD", 100.0);
        state
            .buy(Order {
                market_id: "m".into(),
                token_id: "t".into(),
                price: 0.4,
                size: 10.0,
            })
            .unwrap();
        let fill = state
            .sell(Order {
                market_id: "m".into(),
                token_id: "t".into(),
                price: 0.6,
                size: 5.0,
            })
            .unwrap();
        assert!((fill.realized_pnl - 1.0).abs() < 1e-9);
        assert_eq!(state.cash, 99.0);
        assert!(state
            .sell(Order {
                market_id: "m".into(),
                token_id: "t".into(),
                price: 0.6,
                size: 6.0
            })
            .is_err());
    }
}
