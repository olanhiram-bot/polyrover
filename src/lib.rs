//! Read-only Polymarket SDK and CLI.
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
pub mod auth;
#[cfg(feature = "bridge")]
pub mod bridge;
pub mod capabilities;
#[cfg(feature = "public")]
mod client;
#[cfg(feature = "public")]
pub mod clob;
#[cfg(feature = "execution")]
pub mod clob_orders;
pub mod config;
#[cfg(feature = "public")]
pub mod data;
pub mod data_types;
pub mod error;
#[cfg(feature = "public")]
pub mod gamma;
pub mod intel;
pub mod jsonx;
#[cfg(feature = "public")]
pub mod market_data;
#[cfg(feature = "public")]
pub mod market_resolver;
#[cfg(feature = "public")]
pub mod market_results;
pub mod output;
pub mod paper;
pub mod simulation;
#[cfg(feature = "public")]
pub mod stream;
#[cfg(feature = "public")]
pub mod stream_client;
#[cfg(feature = "public")]
pub mod transport;
pub mod types;
#[cfg(feature = "authenticated")]
pub mod user_stream;
#[cfg(feature = "wallet")]
pub mod wallet;

#[cfg(feature = "public")]
pub use client::{Client, ClientConfig, ClientHealth};
pub use error::{Error, Result};
