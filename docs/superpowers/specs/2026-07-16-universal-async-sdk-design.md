# Universal Async Polyrover SDK Design

Date: 2026-07-16
Status: Approved design

## Summary

Convert Polyrover from a blocking, read-only-oriented SDK into a universal,
async-only Rust interface to Polymarket. This migration replaces blocking HTTP
and WebSocket transports in place, preserves existing behavior, and organizes
capabilities behind layered Cargo features with public data as the safe
default.

This slice does not add fund-moving behavior. MegaBot continues to compile only
Polyrover's public capability, while Polygolem remains MegaBot's exclusive
signing and execution boundary.

## Goals

- Replace blocking `reqwest` with async `reqwest`.
- Replace synchronous Tungstenite transport with `tokio-tungstenite`.
- Preserve existing network method names while making them async.
- Keep DTOs, parsing, simulation, signing helpers, and book math synchronous.
- Retain existing auth, user-stream, wallet, bridge, and order surfaces.
- Gate capabilities and dependencies consistently with layered Cargo features.
- Update the CLI to run on Tokio.
- Update `rust-crypto-data` to await Polyrover directly while retaining
  dedicated supervised Tokio tasks for transport ownership.
- Preserve DTO, parsing, reconnect, deduplication, ordering, statistics, and
  error semantics with TDD parity tests.
- Document implemented and missing coverage in a versioned endpoint/capability
  matrix.

## Non-goals

- A blocking compatibility facade or temporary public blocking aliases.
- Parallel blocking and async transport implementations.
- New live order submission or cancellation.
- Private-key signing, key discovery, import, or storage.
- Relayer or bridge execution.
- Expanding existing authenticated, wallet, execution, or bridge behavior.
- Changing MegaBot's execution ownership from Polygolem to Polyrover.
- Treating Cargo features as authorization or runtime security controls.
- Generating or validating the endpoint matrix with JSON, YAML, or custom
  tooling.

Live execution requires a separate safety design, architecture approval, and
implementation plan.

## Architecture and ownership

Polyrover becomes a universal standalone SDK with a feature-free core and
layered capabilities:

```text
core: DTOs, errors, config, parsing, pure utilities
 ├─ public (default): Gamma, CLOB/Data reads, public WSS, resolution
 ├─ authenticated → public: L2 auth helpers and user WSS
 ├─ wallet: current pure address/readiness helpers
 ├─ execution → authenticated + wallet: current DTOs/unsupported guards
 └─ bridge: current DTOs/unsupported guards
full → execution + bridge
```

All existing network operations use async `reqwest` or
`tokio-tungstenite`. Pure calculations and DTO transformations remain
synchronous. No blocking compatibility layer remains.

Polyrover owns its universal async SDK decision in an ADR inside the Polyrover
repository. MegaBot's root `CONTEXT.md` and `AGENTS.md` state only that
Polyrover is broader standalone software while MegaBot consumes its public
feature exclusively.

Cargo features limit compilation and dependency exposure, not authority.
MegaBot components may compile only Polyrover's public capability; Polygolem
remains MegaBot's exclusive signing and execution boundary.

## Cargo feature contract

```toml
[features]
default = ["public"]
public = []
authenticated = ["public"]
wallet = []
execution = ["authenticated", "wallet"]
bridge = []
full = ["execution", "bridge"]
```

Shared DTOs, errors, configuration, and pure utilities remain in the
feature-free core. Optional network and cryptographic dependencies belong to
the capability that uses them.

Module exports and CLI commands use matching `cfg(feature = "...")` gates.
Capability reporting is feature-aware and must not advertise modules excluded
from the current build.

The existing network-oriented CLI requires the public feature, so core-only
checks skip it cleanly:

```toml
[[bin]]
name = "polyrover"
path = "src/main.rs"
required-features = ["public"]
```

`rust-crypto-data` declares the dependency exactly as follows:

```toml
polyrover = { path = "../polyrover", default-features = false, features = ["public"] }
```

A contract test protects this exact MegaBot dependency boundary.

## Components and API shape

### Feature-free core

The feature-free core contains shared DTOs, errors, configuration values,
JSON/output helpers, capability metadata, parsing, simulation, paper state, and
book math. These remain synchronous.

### Capability modules

- `public`: the unified `Client`, Gamma, public CLOB and Data API clients,
  public market streams, market resolution/results, and public market-data
  tracking.
- `authenticated`: L2 auth helpers and the authenticated user stream.
- `wallet`: existing address derivation and readiness helpers.
- `execution`: existing CLOB order/cancel DTOs and unsupported-operation guards.
- `bridge`: existing bridge DTOs and unsupported-operation guards.

Bridge stays independent while it contains only DTOs and guards. It gains a
wallet dependency only when separately approved concrete behavior requires it.
Arbitrary feature combinations are not a supported test target; only the
meaningful tiers in this design are validated.

### Async network API

Existing network method names remain, but become `async fn`:

- shared HTTP transport methods;
- Gamma, CLOB, Data API, and unified client network methods;
- network-backed market discovery and market-result resolution;
- public and authenticated WebSocket connect, subscribe, read, ping,
  reconnect, and close methods.

The CLI uses `#[tokio::main]` and awaits network commands. Network-backed
simulation remains async at the facade because it fetches a book first;
the pure book simulation function remains synchronous.

`MarketWsClient` retains its pull-based, single-owner `&mut self` API. No
callback framework, generic provider interface, or transport abstraction is
introduced.

## Data flow

### Polyrover

```text
async caller
  → feature-gated client
  → async reqwest / tokio-tungstenite
  → existing synchronous parsing and normalization
  → unchanged Result<T> and stream-status semantics
```

A WebSocket read internally selects between socket input and heartbeat or
reconnect deadlines. Heartbeats and silent-peer detection therefore do not
depend on incoming market traffic. Reconnect preserves subscriptions, tracker
state, deduplication state, ordering, and counters.

### rust-crypto-data

The blocking CLOB coordinator becomes an async coordinator:

- market refresh runs in a supervised Tokio task;
- REST reconciliation runs in a supervised Tokio task using async timers;
- WebSocket ownership remains a dedicated supervised Tokio task;
- market-result resolution awaits Polyrover directly;
- bounded Tokio channels replace Polyrover-specific thread channels;
- Polyrover-specific `spawn_blocking`, `std::thread::sleep`, runtime-handle
  re-entry, and blocking receive loops are removed.

Dedicated async tasks remain appropriate for stream ownership, reconnects, and
channels. The collector continues to own adaptive policy, persistence, feature
publication, transport-health state, fallback decisions, and task supervision.
Polyrover transport tasks report observations but do not acquire collector
policy or persistence responsibilities.

## Backpressure, supervision, and shutdown

Bounded channels intentionally replace the current unbounded collector
channels under this fail-closed contract:

- Every bounded observation send has a timeout shorter than the configured
  heartbeat deadline.
- Configuration validates `send_timeout < heartbeat_deadline`; tests protect
  this invariant.
- A closed receiver means normal shutdown.
- A send timeout returns a typed saturation error to the supervising
  coordinator.
- The supervisor records queue-pressure health, terminates the affected
  transport session, and initiates reconnect or reconciliation.
- Coverage remains incomplete from the last accepted observation until
  authoritative recovery.
- REST snapshots may restore current book state but must not claim recovery of
  missing trades.
- No path silently drops, overwrites, forward-fills, or blocks beyond heartbeat
  deadlines.
- All transport tasks are supervised through owned Tokio `JoinHandle`s.
- Normal shutdown attempts a bounded WebSocket close handshake. Forced
  cancellation may drop the socket.

This policy preserves liveness without presenting invisible market-data loss
as complete evidence.

## Error behavior

Existing error variants and observable behavior remain stable, including:

- HTTP and network failures;
- API status and response body reporting;
- `Retry-After` preservation for rate limits;
- invalid configuration and malformed protocol payloads;
- WebSocket failures and reconnect exhaustion;
- deduplication and invalid-frame accounting;
- existing unsupported-operation guards.

The collector adds a typed channel-saturation failure at its async transport
boundary. Secrets, auth headers, signed material, and payloads remain redacted.
This migration introduces no new secret-bearing network operation.

## Endpoint/capability matrix

Create `docs/endpoint-capability-matrix.md` inside Polyrover with:

```markdown
Matrix schema: 1
Last verified: 2026-07-16
```

The table columns are:

| Surface | Method/event | Endpoint/channel | Transport | Auth level | Cargo feature | Status | Rust API | Test |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |

Allowed statuses are `implemented`, `dto-only`, `unsupported`, and `planned`.
The matrix has separate public, authenticated, wallet, execution, and bridge
sections. Every implemented row links to source and a parity or contract test,
and upstream Polymarket documentation is linked where available. DTO-only or
guarded behavior is never labeled implemented.

Capability changes update the matrix in the same commit. Live source and
manifests remain authoritative. No machine-readable generation or validation
is added until a concrete consumer requires it.

## Documentation changes

Inside Polyrover:

- add an ADR for the universal async SDK boundary and safe-default features;
- add `docs/endpoint-capability-matrix.md`;
- update the README and port roadmap to distinguish available behavior from
  planned universal coverage;
- document the async-only API as an accepted pre-1.0 breaking change.

At the MegaBot root:

- update `CONTEXT.md` and `AGENTS.md` to describe Polyrover as broader
  standalone software while MegaBot compiles only `public`;
- retain Polygolem as MegaBot's sole signing and execution boundary.

No root ADR is needed because MegaBot's execution ownership does not change.

## TDD migration sequence

Each production behavior change starts with the smallest failing parity or
contract test. Each commit compiles and is independently reviewable.

1. Add Cargo feature gates and feature-aware capability-reporting tests.
2. Convert HTTP transport tests, then HTTP clients and network-backed helpers.
3. Convert public and authenticated WebSocket lifecycle tests and transports.
4. Convert the CLI to Tokio and gate the binary on `public`.
5. Adapt collector discovery, REST, WebSocket, and result tasks.
6. Add deterministic tests for silent-peer heartbeat, reconnect deadlines,
   send saturation, supervision, shutdown, and explicit coverage gaps.
7. Remove blocking dependencies and prove no Polyrover-specific blocking
   bridge remains.
8. Update the endpoint matrix and user-facing documentation with actual
   coverage.

Tokio's `test-util` feature is enabled only for tests that require paused time.
Feature-specific integration tests use matching `cfg` gates so core-only and
bridge-only builds do not compile public tests. Tests use local HTTP/WebSocket
fixtures only and never live endpoints or credentials.

## Validation

Polyrover:

```bash
cargo fmt --all -- --check
cargo check --lib --no-default-features
cargo test --no-default-features --features public
cargo test --no-default-features --features authenticated
cargo test --no-default-features --features execution
cargo test --no-default-features --features bridge
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

Collector integration from the Polyrover repository:

```bash
cargo test --manifest-path ../rust-crypto-data/Cargo.toml
cargo clippy --manifest-path ../rust-crypto-data/Cargo.toml --all-targets -- -D warnings
```

Reference checks also prove that:

- `rust-crypto-data` enables exactly `default-features = false` and
  `features = ["public"]`;
- collector production paths contain no Polyrover-specific `spawn_blocking`,
  blocking transport threads, or imports from non-public capability modules;
- capability reporting matches the active feature tier.

## Delivery order

Polyrover changes are committed within the Polyrover repository first. The
Polyrover commit is pushed before the parent MegaBot repository references that
submodule commit. The parent updates its collector integration, documentation,
and submodule pointer only after coordinated Polyrover feature-tier and
collector validation succeeds. Unrelated parent or submodule worktree changes
are not modified.

## Completion criteria

- All existing network operations are native async and preserve their method
  names and observable semantics.
- Blocking `reqwest` and synchronous Tungstenite transport are absent.
- No public blocking facade or wrapper remains.
- Pure DTO, parsing, simulation, signing-helper, and book-math APIs remain
  synchronous.
- All meaningful Cargo feature tiers compile and pass their tests.
- Capability reporting reflects only compiled capabilities.
- The CLI runs on Tokio and requires `public`.
- `rust-crypto-data` compiles only `public`, uses supervised Tokio transport
  tasks, and contains no Polyrover-specific blocking bridge.
- Saturation, heartbeat, reconnect, shutdown, and coverage-gap behavior are
  deterministic and regression-tested.
- The endpoint matrix accurately separates implemented, DTO-only, unsupported,
  and planned behavior.
- No new order, cancellation, private-key, relayer, or bridge execution path is
  introduced.
- Polygolem remains MegaBot's exclusive signing and execution boundary.
