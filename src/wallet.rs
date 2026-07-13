//! Polygon chain and proxy-factory constants plus wallet readiness
//! reporting (read-only; no key handling).

use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

use crate::{Error, Result};

pub const POLYGON_CHAIN_ID: i64 = 137;
pub const PROXY_FACTORY_ADDR: &str = "0xaB45c5A4B0c941a2F231C04C3f49182e1A254052";
pub const SAFE_FACTORY_ADDR: &str = "0xaacFeEa03eb1561C4e67d661e40682Bd20E3541b";
const DEPOSIT_WALLET_FACTORY_ADDRESS: &str = "0x00000000000Fb5C9ADea0298D729A0CB3823Cc07";
const DEPOSIT_WALLET_BEACON_ADDRESS: &str = "0x7a18EDfE055488A3128f01F563E5B479D92FFc3A";
const PROXY_INIT_CODE_HASH: &str =
    "0xd21df8dc65880a8606f09fe0ce3df9b8869287ab0b058be05aa9e8af6330a00b";
const SAFE_INIT_CODE_HASH: &str =
    "0x2bce2127ff07fb632d16c8347c4ebf501f4841168bed00d9e6ef715ddb6fcecf";
const ERC1967_BEACON_PREFIX: &str = "0x6100523D8160233D3973";
const ERC1967_BEACON_CONST3: &str = "0x60195155f3363d3d373d3d363d602036600436635c60da";
const ERC1967_BEACON_CONST2: &str =
    "0x1b60e01b36527fa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6c";
const ERC1967_BEACON_CONST1: &str =
    "0xb3582b35133d50545afa5036515af43d6000803e604d573d6000fd5b3d6000f3";

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadyInfo {
    pub chain_id: i64,
    pub eoa: String,
    pub deposit_wallet: String,
    pub proxy_wallet: String,
    pub safe_wallet: String,
    pub has_signer: bool,
}

pub fn derive_deposit_wallet(eoa: &str) -> Result<String> {
    let owner = address_bytes(eoa).ok_or_else(|| Error::Invalid("invalid EOA address".into()))?;
    Ok(create2(
        &address_bytes(DEPOSIT_WALLET_FACTORY_ADDRESS).unwrap(),
        &deposit_salt(&owner),
        &keccak(&deposit_wallet_beacon_init_code(&owner)),
    ))
}

pub fn derive_proxy_wallet(eoa: &str) -> String {
    let salt = proxy_salt(eoa);
    create2(
        &address_bytes(PROXY_FACTORY_ADDR).unwrap(),
        &salt,
        &hex_bytes(PROXY_INIT_CODE_HASH),
    )
}

pub fn derive_safe_wallet(eoa: &str) -> String {
    let salt = safe_salt(eoa);
    create2(
        &address_bytes(SAFE_FACTORY_ADDR).unwrap(),
        &salt,
        &hex_bytes(SAFE_INIT_CODE_HASH),
    )
}

pub fn readiness(chain_id: i64, eoa: &str) -> ReadyInfo {
    let mut info = ReadyInfo {
        chain_id,
        eoa: eoa.into(),
        ..Default::default()
    };
    if !eoa.is_empty() {
        info.has_signer = true;
        info.deposit_wallet = derive_deposit_wallet(eoa).unwrap_or_default();
        info.proxy_wallet = derive_proxy_wallet(eoa);
        info.safe_wallet = derive_safe_wallet(eoa);
    }
    info
}

fn deposit_salt(owner: &[u8; 20]) -> [u8; 32] {
    let factory = address_bytes(DEPOSIT_WALLET_FACTORY_ADDRESS).unwrap();
    let mut wallet_id = [0u8; 32];
    wallet_id[12..].copy_from_slice(owner);
    let mut packed = Vec::with_capacity(64);
    packed.extend_from_slice(&left_pad_address(&factory));
    packed.extend_from_slice(&wallet_id);
    keccak(&packed)
}

fn deposit_wallet_beacon_init_code(owner: &[u8; 20]) -> Vec<u8> {
    let beacon = address_bytes(DEPOSIT_WALLET_BEACON_ADDRESS).unwrap();
    let factory = address_bytes(DEPOSIT_WALLET_FACTORY_ADDRESS).unwrap();
    let mut wallet_id = [0u8; 32];
    wallet_id[12..].copy_from_slice(owner);
    let mut args = Vec::with_capacity(64);
    args.extend_from_slice(&left_pad_address(&factory));
    args.extend_from_slice(&wallet_id);

    let mut prefix = hex_bytes(ERC1967_BEACON_PREFIX);
    let args_len = (args.len() as u64) << 56;
    for (i, b) in args_len.to_be_bytes().iter().enumerate() {
        prefix[2 + i] = prefix[2 + i].wrapping_add(*b);
    }
    let mut out = prefix;
    out.extend_from_slice(&beacon);
    out.extend_from_slice(&hex_bytes(ERC1967_BEACON_CONST3));
    out.extend_from_slice(&hex_bytes(ERC1967_BEACON_CONST2));
    out.extend_from_slice(&hex_bytes(ERC1967_BEACON_CONST1));
    out.extend_from_slice(&args);
    out
}

fn proxy_salt(eoa: &str) -> [u8; 32] {
    address_bytes(eoa).map(|a| keccak(&a)).unwrap_or([0u8; 32])
}
fn safe_salt(eoa: &str) -> [u8; 32] {
    address_bytes(eoa)
        .map(|a| {
            let mut padded = [0u8; 32];
            padded[12..].copy_from_slice(&a);
            keccak(&padded)
        })
        .unwrap_or([0u8; 32])
}

fn create2(factory: &[u8; 20], salt: &[u8; 32], init_code_hash: &[u8]) -> String {
    let mut raw = Vec::with_capacity(1 + 20 + 32 + init_code_hash.len());
    raw.push(0xff);
    raw.extend_from_slice(factory);
    raw.extend_from_slice(salt);
    raw.extend_from_slice(init_code_hash);
    format!("0x{}", hex::encode(&keccak(&raw)[12..]))
}

fn left_pad_address(address: &[u8; 20]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[12..].copy_from_slice(address);
    out
}
fn keccak(data: &[u8]) -> [u8; 32] {
    let digest = Keccak256::digest(data);
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}
fn address_bytes(value: &str) -> Option<[u8; 20]> {
    let bytes = hex::decode(strip_0x(value)).ok()?;
    (bytes.len() == 20).then(|| {
        let mut out = [0u8; 20];
        out.copy_from_slice(&bytes);
        out
    })
}
fn hex_bytes(value: &str) -> Vec<u8> {
    hex::decode(strip_0x(value)).expect("valid hex constant")
}
fn strip_0x(value: &str) -> &str {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_deposit_wallet_inputs() {
        assert!(derive_deposit_wallet("not-an-address").is_err());
        assert!(
            derive_deposit_wallet("0x0000000000000000000000000000000000000001")
                .unwrap()
                .starts_with("0x")
        );
    }

    #[test]
    fn readiness_does_not_require_private_key() {
        let info = readiness(
            POLYGON_CHAIN_ID,
            "0x0000000000000000000000000000000000000001",
        );
        assert!(info.has_signer);
        assert_eq!(info.chain_id, 137);
        assert!(info.deposit_wallet.starts_with("0x"));
        assert!(info.proxy_wallet.starts_with("0x"));
        assert!(info.safe_wallet.starts_with("0x"));
    }
}
