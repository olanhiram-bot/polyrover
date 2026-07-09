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
    let mut caps = vec![
        cap("bridge.funding", "Bridge", "Supported assets, deposit addresses, quotes, and deposit status for pUSD funding.", false, true, vec![AuthRequirement::None], WalletMode::DepositWalletOnly, &["pkg/bridge"], &["bridge assets", "bridge deposit", "bridge status", "bridge quote"]),
        cap("clob.public_data", "CLOB API", "Public order books, prices, spreads, tick sizes, and market metadata.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/clob", "pkg/orderbook", "pkg/marketdata"], &["book", "exchange book", "exchange markets", "exchange price-history"]),
        cap("clob.trading", "CLOB API", "Deposit-wallet CLOB V2 order signing, placement, cancellation, account reads, and builder attribution.", false, true, vec![AuthRequirement::L1, AuthRequirement::L2, AuthRequirement::PrivateKey], WalletMode::DepositWalletOnly, &["pkg/clob"], &["exchange create-order", "exchange market-order", "exchange cancel"]),
        cap("data.positions", "Data API", "Public wallet-level positions, activity, trades, value, holders, leaderboard, and open interest.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/data"], &["analytics positions", "analytics trades", "analytics activity"]),
        cap("gamma.markets", "Gamma API", "Public event, market, tag, series, comment, and search discovery.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/gamma", "pkg/universal"], &["markets search", "markets markets", "markets market"]),
        cap("relayer.deposit_wallet", "Relayer V2", "Deposit-wallet deploy, approvals, gasless transactions, CTF redeem, and transaction lookup.", false, true, vec![AuthRequirement::Siwe, AuthRequirement::PrivateKey], WalletMode::DepositWalletOnly, &["pkg/relayer", "pkg/ctf", "pkg/settlement"], &["wallet", "tx transaction"]),
        cap("websocket.market", "CLOB WebSocket", "Public real-time book, price, last-trade, tick-size, best-bid-ask, and lifecycle events.", true, false, vec![AuthRequirement::None], WalletMode::None, &["pkg/stream", "pkg/marketdata"], &["stream market", "stream crypto", "marketdata live"]),
        cap("websocket.user", "CLOB WebSocket", "Authenticated user order and trade stream for inspection and reconciliation.", false, false, vec![AuthRequirement::L2], WalletMode::DepositWalletOnly, &["pkg/stream"], &["stream user"]),
    ];
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
    fn includes_critical_surfaces_and_sorted() {
        let caps = all();
        for id in [
            "gamma.markets",
            "clob.public_data",
            "clob.trading",
            "data.positions",
            "relayer.deposit_wallet",
            "bridge.funding",
            "websocket.market",
            "websocket.user",
        ] {
            assert!(caps.iter().any(|c| c.id == id), "missing {id}");
        }
        assert!(caps.windows(2).all(|w| w[0].id <= w[1].id));
    }

    #[test]
    fn trading_declares_auth_and_read_only_excludes_secrets() {
        let caps = all();
        let trading = caps.iter().find(|c| c.id == "clob.trading").unwrap();
        assert!(trading.mutating);
        assert!(trading.requires(AuthRequirement::L1));
        assert!(trading.requires(AuthRequirement::L2));
        assert!(trading.requires(AuthRequirement::PrivateKey));
        for cap in caps.iter().filter(|c| c.read_only) {
            assert!(!cap.mutating);
            assert!(!cap.requires(AuthRequirement::L2));
            assert!(!cap.requires(AuthRequirement::Siwe));
            assert!(!cap.requires(AuthRequirement::PrivateKey));
        }
    }
}
