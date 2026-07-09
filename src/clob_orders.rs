use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::jsonx::{int_or_zero, string_or_number};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct CreateOrderParams {
    pub token_id: String,
    pub side: String,
    pub price: String,
    pub size: String,
    pub order_type: String,
    pub expiration: String,
    pub post_only: bool,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct MarketOrderParams {
    pub token_id: String,
    pub side: String,
    pub amount: String,
    pub price: String,
    pub order_type: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct OrderPlacementResponse {
    pub success: bool,
    #[serde(default, rename = "orderID", alias = "order_id")]
    pub order_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(
        default,
        rename = "makingAmount",
        alias = "making_amount",
        deserialize_with = "string_or_number"
    )]
    pub making_amount: String,
    #[serde(
        default,
        rename = "takingAmount",
        alias = "taking_amount",
        deserialize_with = "string_or_number"
    )]
    pub taking_amount: String,
    #[serde(default, rename = "errorMsg", alias = "error_msg")]
    pub error_msg: String,
    #[serde(default, rename = "transaction_hash", alias = "transactionHash")]
    pub transaction_hash: String,
    #[serde(
        default,
        rename = "transactionsHashes",
        alias = "transactionHashes",
        alias = "transaction_hashes"
    )]
    pub transactions_hashes: Vec<String>,
    #[serde(default, rename = "tradeIDs", alias = "tradeIds", alias = "trade_ids")]
    pub trade_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct BatchOrderResponse {
    #[serde(default)]
    pub orders: Vec<OrderPlacementResponse>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct CancelOrdersResponse {
    #[serde(default)]
    pub canceled: Vec<String>,
    #[serde(default, rename = "not_canceled", alias = "notCanceled")]
    pub not_canceled: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct CancelMarketParams {
    pub market: String,
    pub asset: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct OrderRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub market: String,
    #[serde(default, alias = "assetId")]
    pub asset_id: String,
    #[serde(default)]
    pub side: String,
    #[serde(default, alias = "originalSize", deserialize_with = "string_or_number")]
    pub original_size: String,
    #[serde(default, alias = "sizeMatched", deserialize_with = "string_or_number")]
    pub size_matched: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub price: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default, rename = "type")]
    pub record_type: String,
    #[serde(default, alias = "orderType", deserialize_with = "string_or_number")]
    pub order_type: String,
    #[serde(default, alias = "signatureType", deserialize_with = "int_or_zero")]
    pub signature_type: i64,
    #[serde(default, alias = "createdAt", deserialize_with = "string_or_number")]
    pub created_at: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub expiration: String,
    #[serde(default, alias = "makerAddress")]
    pub maker_address: String,
    #[serde(default, alias = "associateTrades")]
    pub associate_trades: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct OrderSignaturePreview {
    pub token_id: String,
    pub side: String,
    pub order_type: String,
    pub price: String,
    pub maker_amount: String,
    pub taker_amount: String,
    pub maker: String,
    pub signer: String,
    pub signature_type: i64,
    pub neg_risk: bool,
    pub exchange_contract: String,
    pub wallet_domain: String,
    pub wallet_operation: String,
    pub signature_length: usize,
    pub signature_included: bool,
    pub note: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_placement_accepts_camel_snake_and_numeric_amounts() {
        let row: OrderPlacementResponse = serde_json::from_str(r#"{"success":true,"order_id":"o1","making_amount":12,"takingAmount":"3","transactionHash":"0xabc","tradeIds":["t1"]}"#).unwrap();
        assert!(row.success);
        assert_eq!(row.order_id, "o1");
        assert_eq!(row.making_amount, "12");
        assert_eq!(row.taking_amount, "3");
        assert_eq!(row.transaction_hash, "0xabc");
        assert_eq!(row.trade_ids, vec!["t1"]);
    }

    #[test]
    fn cancel_response_accepts_not_canceled_alias() {
        let row: CancelOrdersResponse =
            serde_json::from_str(r#"{"canceled":["1"],"notCanceled":{"2":"missing"}}"#).unwrap();
        assert_eq!(row.canceled, vec!["1"]);
        assert_eq!(row.not_canceled["2"], "missing");
    }

    #[test]
    fn order_record_accepts_backend_drift() {
        let row: OrderRecord = serde_json::from_str(r#"{"id":"o","assetId":"a","originalSize":5,"sizeMatched":"2","price":0.42,"orderType":"GTC","signatureType":"3","createdAt":123,"associateTrades":["t"]}"#).unwrap();
        assert_eq!(row.asset_id, "a");
        assert_eq!(row.original_size, "5");
        assert_eq!(row.price, "0.42");
        assert_eq!(row.signature_type, 3);
        assert_eq!(row.created_at, "123");
    }
}
