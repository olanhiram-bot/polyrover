//! Catalog of SDK/CLI capabilities with their auth and wallet requirements.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AuthRequirement {
    None,
    L1,
    L2,
    Siwe,
    PrivateKey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WalletMode {
    None,
    DepositWalletOnly,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub service: String,
    pub summary: String,
    pub read_only: bool,
    pub mutating: bool,
    pub auth: Vec<AuthRequirement>,
    pub wallet_mode: WalletMode,
    pub sdk_packages: Vec<String>,
    pub cli: Vec<String>,
}

impl Capability {
    pub fn requires(&self, auth: AuthRequirement) -> bool {
        self.auth.contains(&auth)
    }
}

pub fn all() -> Vec<Capability> {
    let mut caps: Vec<Capability> = Vec::new();

    #[cfg(feature = "public")]
    caps.extend([
        cap("clob.public_data", "CLOB API", "Public order books, prices, spreads, tick sizes, and market metadata.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/clob", "pkg/orderbook", "pkg/marketdata"], &["book", "exchange book", "exchange markets", "exchange price-history"]),
        cap("data.positions", "Data API", "Public wallet-level positions, activity, trades, value, holders, leaderboard, and open interest.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/data"], &["analytics positions", "analytics trades", "analytics activity"]),
        cap("gamma.markets", "Gamma API", "Public event, market, tag, series, comment, and search discovery.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/gamma", "pkg/universal"], &["markets search", "markets markets", "markets market"]),
        cap("websocket.market", "CLOB WebSocket", "Public real-time book, price, last-trade, tick-size, best-bid-ask, and lifecycle events.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/stream", "pkg/marketdata"], &["stream market", "stream crypto", "marketdata live"]),
    ]);
    #[cfg(feature = "authenticated")]
    caps.push(cap(
        "websocket.user",
        "CLOB WebSocket",
        "Authenticated user order and trade stream for inspection and reconciliation.",
        false,
        false,
        vec![AuthRequirement::L2],
        WalletMode::DepositWalletOnly,
        &["pkg/stream"],
        &["stream user"],
    ));
    #[cfg(feature = "wallet")]
    caps.push(cap("relayer.deposit_wallet", "Relayer V2", "Deposit-wallet deploy, approvals, gasless transactions, CTF redeem, and transaction lookup.", false, true, vec![AuthRequirement::Siwe, AuthRequirement::PrivateKey], WalletMode::DepositWalletOnly, &["pkg/relayer", "pkg/ctf", "pkg/settlement"], &["wallet", "tx transaction"]));
    #[cfg(feature = "execution")]
    caps.push(cap("clob.trading", "CLOB API", "Deposit-wallet CLOB V2 order signing, placement, cancellation, account reads, and builder attribution.", false, true, vec![AuthRequirement::L1, AuthRequirement::L2, AuthRequirement::PrivateKey], WalletMode::DepositWalletOnly, &["pkg/clob"], &["exchange create-order", "exchange market-order", "exchange cancel"]));
    #[cfg(feature = "bridge")]
    caps.push(cap(
        "bridge.funding",
        "Bridge",
        "Supported assets, deposit addresses, quotes, and deposit status for pUSD funding.",
        false,
        true,
        vec![AuthRequirement::None],
        WalletMode::DepositWalletOnly,
        &["pkg/bridge"],
        &[
            "bridge assets",
            "bridge deposit",
            "bridge status",
            "bridge quote",
        ],
    ));

    caps.sort_by(|a, b| a.id.cmp(&b.id));
    caps
}

pub fn read_only_ids() -> Vec<String> {
    all()
        .into_iter()
        .filter(|c| c.read_only)
        .map(|c| c.id)
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn cap(
    id: &str,
    service: &str,
    summary: &str,
    read_only: bool,
    mutating: bool,
    auth: Vec<AuthRequirement>,
    wallet_mode: WalletMode,
    sdk: &[&str],
    cli: &[&str],
) -> Capability {
    Capability {
        id: id.into(),
        service: service.into(),
        summary: summary.into(),
        read_only,
        mutating,
        auth,
        wallet_mode,
        sdk_packages: sdk.iter().map(|s| s.to_string()).collect(),
        cli: cli.iter().map(|s| s.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_only_compiled_surfaces_and_stays_sorted() {
        let caps = all();
        let ids = caps.iter().map(|cap| cap.id.as_str()).collect::<Vec<_>>();
        #[cfg(feature = "public")]
        for id in [
            "gamma.markets",
            "clob.public_data",
            "data.positions",
            "websocket.market",
        ] {
            assert!(ids.contains(&id), "missing {id}");
        }
        #[cfg(feature = "authenticated")]
        assert!(ids.contains(&"websocket.user"));
        #[cfg(feature = "wallet")]
        assert!(ids.contains(&"relayer.deposit_wallet"));
        #[cfg(feature = "execution")]
        assert!(ids.contains(&"clob.trading"));
        #[cfg(feature = "bridge")]
        assert!(ids.contains(&"bridge.funding"));
        assert!(caps.windows(2).all(|pair| pair[0].id <= pair[1].id));
    }

    #[test]
    fn read_only_capabilities_exclude_secret_requirements() {
        for cap in all().iter().filter(|cap| cap.read_only) {
            assert!(!cap.mutating);
            assert!(!cap.requires(AuthRequirement::L2));
            assert!(!cap.requires(AuthRequirement::Siwe));
            assert!(!cap.requires(AuthRequirement::PrivateKey));
        }
    }

    #[cfg(feature = "execution")]
    #[test]
    fn trading_declares_explicit_auth() {
        let caps = all();
        let trading = caps.iter().find(|cap| cap.id == "clob.trading").unwrap();
        assert!(trading.mutating);
        assert!(trading.requires(AuthRequirement::L1));
        assert!(trading.requires(AuthRequirement::L2));
        assert!(trading.requires(AuthRequirement::PrivateKey));
    }
}
