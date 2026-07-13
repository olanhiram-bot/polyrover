//! Deposit-address and asset metadata types for the Polymarket bridge API.

use serde::{Deserialize, Serialize};

use crate::{
    jsonx::{float_or_zero, int_or_zero, string_or_number},
    Error, Result,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct DepositAddress {
    pub evm: String,
    pub svm: String,
    pub btc: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct CreateDepositAddressResponse {
    pub address: DepositAddress,
    pub note: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct TokenInfo {
    #[serde(default, deserialize_with = "string_or_number")]
    pub name: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub symbol: String,
    #[serde(default, deserialize_with = "string_or_number")]
    pub address: String,
    #[serde(default, deserialize_with = "int_or_zero")]
    pub decimals: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct SupportedAsset {
    #[serde(default, rename = "chainId", deserialize_with = "string_or_number")]
    pub chain_id: String,
    #[serde(default, rename = "chainName", deserialize_with = "string_or_number")]
    pub chain_name: String,
    pub token: TokenInfo,
    #[serde(default, rename = "minCheckoutUsd", deserialize_with = "float_or_zero")]
    pub min_checkout_usd: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct SupportedAssetsResponse {
    #[serde(default, rename = "supportedAssets")]
    pub supported_assets: Vec<SupportedAsset>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct DepositTransaction {
    #[serde(default, rename = "fromChainId", deserialize_with = "string_or_number")]
    pub from_chain_id: String,
    #[serde(
        default,
        rename = "fromTokenAddress",
        deserialize_with = "string_or_number"
    )]
    pub from_token_address: String,
    #[serde(
        default,
        rename = "fromAmountBaseUnit",
        deserialize_with = "string_or_number"
    )]
    pub from_amount_base_unit: String,
    #[serde(default, rename = "toChainId", deserialize_with = "string_or_number")]
    pub to_chain_id: String,
    #[serde(
        default,
        rename = "toTokenAddress",
        deserialize_with = "string_or_number"
    )]
    pub to_token_address: String,
    #[serde(default, rename = "txHash", deserialize_with = "string_or_number")]
    pub tx_hash: String,
    #[serde(default, rename = "createdTimeMs", deserialize_with = "int_or_zero")]
    pub created_time_ms: i64,
    #[serde(default, deserialize_with = "string_or_number")]
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct DepositStatusResponse {
    #[serde(default)]
    pub transactions: Vec<DepositTransaction>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct QuoteRequest {
    #[serde(rename = "fromAmountBaseUnit")]
    pub from_amount_base_unit: String,
    #[serde(rename = "fromChainId")]
    pub from_chain_id: String,
    #[serde(rename = "fromTokenAddress")]
    pub from_token_address: String,
    #[serde(rename = "recipientAddress")]
    pub recipient_address: String,
    #[serde(rename = "toChainId")]
    pub to_chain_id: String,
    #[serde(rename = "toTokenAddress")]
    pub to_token_address: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct FeeBreakdown {
    #[serde(default, rename = "appFeeLabel", deserialize_with = "string_or_number")]
    pub app_fee_label: String,
    #[serde(default, rename = "appFeePercent", deserialize_with = "float_or_zero")]
    pub app_fee_percent: f64,
    #[serde(default, rename = "gasUsd", deserialize_with = "float_or_zero")]
    pub gas_usd: f64,
    #[serde(default, rename = "minReceived", deserialize_with = "float_or_zero")]
    pub min_received: f64,
    #[serde(default, rename = "totalImpactUsd", deserialize_with = "float_or_zero")]
    pub total_impact_usd: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct QuoteResponse {
    #[serde(
        default,
        rename = "estCheckoutTimeMs",
        deserialize_with = "int_or_zero"
    )]
    pub est_checkout_time_ms: i64,
    #[serde(default, rename = "estFeeBreakdown")]
    pub est_fee_breakdown: FeeBreakdown,
    #[serde(default, rename = "estInputUsd", deserialize_with = "float_or_zero")]
    pub est_input_usd: f64,
    #[serde(default, rename = "estOutputUsd", deserialize_with = "float_or_zero")]
    pub est_output_usd: f64,
    #[serde(
        default,
        rename = "estToTokenBaseUnit",
        deserialize_with = "string_or_number"
    )]
    pub est_to_token_base_unit: String,
    #[serde(default, rename = "quoteId", deserialize_with = "string_or_number")]
    pub quote_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct WithdrawRequest {
    #[serde(rename = "fromChainId")]
    pub from_chain_id: String,
    #[serde(rename = "fromTokenAddress")]
    pub from_token_address: String,
    #[serde(rename = "fromAmountBaseUnit")]
    pub from_amount_base_unit: String,
    #[serde(rename = "toChainId")]
    pub to_chain_id: String,
    #[serde(rename = "toTokenAddress")]
    pub to_token_address: String,
    #[serde(rename = "recipientAddress")]
    pub recipient_address: String,
    #[serde(rename = "quoteId", skip_serializing_if = "String::is_empty")]
    pub quote_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WithdrawDryRun {
    pub request: WithdrawRequest,
    #[serde(rename = "readyToSubmit")]
    pub ready_to_submit: bool,
    pub unsupported: bool,
    #[serde(rename = "safetyWarnings")]
    pub safety_warnings: Vec<String>,
}

pub fn build_withdraw_dry_run(request: WithdrawRequest) -> Result<WithdrawDryRun> {
    validate_withdraw(&request)?;
    Ok(WithdrawDryRun { request, ready_to_submit: false, unsupported: true, safety_warnings: vec![
        "bridge withdrawal/offramp live submission is intentionally disabled".into(),
        "verify recipient address, token, chain, quote, and custody route before any future submit path".into(),
        "use explicit operator confirmation before moving funds".into(),
    ]})
}

fn validate_withdraw(req: &WithdrawRequest) -> Result<()> {
    for (field, value) in [
        ("fromChainId", &req.from_chain_id),
        ("fromTokenAddress", &req.from_token_address),
        ("fromAmountBaseUnit", &req.from_amount_base_unit),
        ("toChainId", &req.to_chain_id),
        ("toTokenAddress", &req.to_token_address),
        ("recipientAddress", &req.recipient_address),
    ] {
        if value.trim().is_empty() {
            return Err(Error::Invalid(format!("{field} is required")));
        }
    }
    let amount: i128 = req.from_amount_base_unit.trim().parse().map_err(|_| {
        Error::Invalid("fromAmountBaseUnit must be a positive base-10 integer".into())
    })?;
    if amount <= 0 {
        return Err(Error::Invalid(
            "fromAmountBaseUnit must be a positive base-10 integer".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_numbers_accept_strings_or_numbers() {
        let asset: SupportedAsset = serde_json::from_str(r#"{"chainId":137,"chainName":"Polygon","token":{"name":"pUSD","symbol":"pUSD","address":"0x1","decimals":"6"},"minCheckoutUsd":"1.5"}"#).unwrap();
        assert_eq!(asset.chain_id, "137");
        assert_eq!(asset.token.decimals, 6);
        assert_eq!(asset.min_checkout_usd, 1.5);
    }

    #[test]
    fn withdraw_dry_run_never_ready_to_submit() {
        let req = WithdrawRequest {
            from_chain_id: "137".into(),
            from_token_address: "0xfrom".into(),
            from_amount_base_unit: "100".into(),
            to_chain_id: "1".into(),
            to_token_address: "0xto".into(),
            recipient_address: "0xrecipient".into(),
            quote_id: "q".into(),
        };
        let dry = build_withdraw_dry_run(req).unwrap();
        assert!(dry.unsupported);
        assert!(!dry.ready_to_submit);
        assert!(dry.safety_warnings.iter().any(|w| w.contains("disabled")));
        assert!(build_withdraw_dry_run(WithdrawRequest::default()).is_err());
    }
}
