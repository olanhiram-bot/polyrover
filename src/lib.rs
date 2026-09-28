//! Universal async Polymarket SDK and CLI with a safe public-data default.
//!
//! HTTP clients: [`gamma`] (market/event discovery), [`clob`] (order books,
//! prices), [`data`] (positions, trades, activity), unified behind
//! [`Client`]. Streaming: [`stream`] (market WSS decoding),
//! [`stream_client`] (subscription lifecycle, ping, reconnect, dedup),
//! [`user_stream`] (user WSS shapes). Domain: [`types`],
//! [`market_resolver`] (crypto window discovery, up/down token resolution),
//! [`market_data`] (book state, top-of-book, liquidity, depth),
//! [`market_results`] (authoritative outcomes). Local research: [`paper`],
//! [`simulation`]. Support: [`auth`], [`wallet`], [`capabilities`],
//! [`config`], [`error`], [`jsonx`], [`output`], [`transport`]. The CLI
//! entrypoint lives in `src/main.rs`.

#[cfg(feature = "authenticated")]
#[path = "capabilities/auth.rs"]
pub mod auth;
#[cfg(feature = "authenticated")]
#[path = "api/authenticated_clob.rs"]
pub mod authenticated_clob;
#[cfg(feature = "bridge")]
#[path = "capabilities/bridge.rs"]
pub mod bridge;
#[path = "capabilities/capabilities.rs"]
pub mod capabilities;
#[cfg(feature = "public")]
mod client;
#[cfg(feature = "public")]
#[path = "api/clob.rs"]
pub mod clob;
#[cfg(feature = "authenticated")]
#[path = "capabilities/clob_history.rs"]
pub mod clob_history;
#[cfg(feature = "execution")]
#[path = "capabilities/clob_orders.rs"]
pub mod clob_orders;
#[path = "streaming/config.rs"]
pub mod config;
#[cfg(feature = "public")]
#[path = "api/crypto_price.rs"]
pub mod crypto_price;
#[cfg(feature = "public")]
#[path = "api/data.rs"]
pub mod data;
#[path = "models/data_types.rs"]
pub mod data_types;
pub mod error;
#[cfg(feature = "public")]
#[path = "api/gamma.rs"]
pub mod gamma;
#[path = "research/intel.rs"]
pub mod intel;
#[path = "models/jsonx.rs"]
pub mod jsonx;
#[cfg(feature = "public")]
#[path = "streaming/market_data.rs"]
pub mod market_data;
#[cfg(feature = "public")]
#[path = "research/market_flow.rs"]
pub mod market_flow;
#[cfg(feature = "public")]
#[path = "research/market_resolver.rs"]
pub mod market_resolver;
#[cfg(feature = "public")]
#[path = "research/market_results.rs"]
pub mod market_results;
#[path = "cli/output.rs"]
pub mod output;
#[path = "research/paper.rs"]
pub mod paper;
#[cfg(feature = "public")]
#[path = "api/query.rs"]
mod query;
#[path = "research/simulation.rs"]
pub mod simulation;
#[cfg(feature = "public")]
#[path = "streaming/stream.rs"]
pub mod stream;
#[cfg(feature = "public")]
#[path = "streaming/stream_client.rs"]
pub mod stream_client;
#[cfg(feature = "public")]
#[path = "api/transport.rs"]
pub mod transport;
#[path = "models/types.rs"]
pub mod types;
#[cfg(feature = "authenticated")]
#[path = "streaming/user_stream.rs"]
pub mod user_stream;
#[cfg(feature = "wallet")]
#[path = "capabilities/wallet.rs"]
pub mod wallet;
#[cfg(feature = "public")]
#[path = "research/wallet_dossier.rs"]
pub mod wallet_dossier;

#[cfg(feature = "public")]
pub use client::{Client, ClientConfig, ClientHealth};
pub use error::{Error, Result};
pub use rust_decimal::Decimal;
