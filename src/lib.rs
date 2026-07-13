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

pub mod auth;
pub mod bridge;
pub mod capabilities;
mod client;
pub mod clob;
pub mod clob_orders;
pub mod config;
pub mod data;
pub mod data_types;
pub mod error;
pub mod gamma;
pub mod intel;
pub mod jsonx;
pub mod market_data;
pub mod market_resolver;
pub mod market_results;
pub mod output;
pub mod paper;
pub mod simulation;
pub mod stream;
pub mod stream_client;
pub mod transport;
pub mod types;
pub mod user_stream;
pub mod wallet;

pub use client::{Client, ClientConfig, ClientHealth};
pub use error::{Error, Result};
