//! `polyrover` CLI entrypoint dispatching to the SDK modules.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use polyrover::{
    gamma, output, paper, simulation, stream, stream_client, Client, ClientConfig, Error, Result,
};
use serde_json::json;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        let body = output::error("polyrover", "error", &err.to_string())
            .unwrap_or_else(|_| format!("error: {err}\n"));
        eprint!("{body}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--json").collect();
    if matches!(args.last().map(String::as_str), Some("-h" | "--help")) {
        return print_command_help(&args[..args.len() - 1]);
    }
    let client = Client::new(ClientConfig::default())?;
    match args.as_slice() {
        [] => {
            print_help();
            Ok(())
        }
        [cmd] if cmd == "help" || cmd == "--help" => {
            print_help();
            Ok(())
        }
        [cmd, rest @ ..] if cmd == "help" => print_command_help(rest),
        [cmd] if cmd == "ping" => ping(&client).await,
        [group, cmd, rest @ ..] if group == "gamma" && cmd == "search" => {
            gamma_search(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "gamma" && cmd == "markets" => {
            gamma_markets(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "book" => {
            clob_book(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "price" => {
            clob_price(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "simulate" => {
            clob_simulate(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "positions" => {
            data_positions(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "trades" => {
            data_trades(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "leaderboard" => {
            data_leaderboard(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "stream" && cmd == "watch" => stream_watch(rest).await,
        [group, cmd, rest @ ..] if group == "sim" && cmd == "reset" => sim_reset(rest),
        [group, cmd, rest @ ..] if group == "sim" && cmd == "buy" => sim_buy(rest),
        [group, cmd, rest @ ..] if group == "sim" && cmd == "sell" => sim_sell(rest),
        _ => Err(unknown_command(&args)),
    }
}

async fn ping(client: &Client) -> Result<()> {
    print_success("ping", client.health().await)
}

async fn gamma_search(client: &Client, args: &[String]) -> Result<()> {
    let query = flag(args, "--query").unwrap_or_default();
    let limit = flag(args, "--limit").and_then(|v| v.parse().ok());
    print_success(
        "gamma search",
        client
            .search(&gamma::SearchParams {
                q: query,
                limit_per_type: limit,
                ..Default::default()
            })
            .await?,
    )
}

async fn gamma_markets(client: &Client, args: &[String]) -> Result<()> {
    let limit = flag(args, "--limit").and_then(|v| v.parse().ok());
    print_success(
        "gamma markets",
        client
            .markets(&gamma::MarketParams {
                limit,
                ..Default::default()
            })
            .await?,
    )
}

async fn clob_book(client: &Client, args: &[String]) -> Result<()> {
    let token = flag(args, "--token-id").unwrap_or_default();
    print_success("clob book", client.order_book(&token).await?)
}

async fn clob_price(client: &Client, args: &[String]) -> Result<()> {
    let token = flag(args, "--token-id").unwrap_or_default();
    let side = flag(args, "--side").unwrap_or_else(|| "buy".into());
    print_success(
        "clob price",
        json!({"price": client.price(&token, &side).await?}),
    )
}

async fn clob_simulate(client: &Client, args: &[String]) -> Result<()> {
    let token = flag(args, "--token")
        .or_else(|| flag(args, "--token-id"))
        .unwrap_or_default();
    let side = flag(args, "--side").unwrap_or_else(|| "buy".into());
    let amount = flag(args, "--amount").unwrap_or_default();
    let limit_price = flag(args, "--limit-price").unwrap_or_default();
    print_success(
        "clob simulate",
        client
            .simulate(simulation::Request {
                token_id: token,
                side,
                amount,
                limit_price,
            })
            .await?,
    )
}

async fn data_positions(client: &Client, args: &[String]) -> Result<()> {
    let user = flag(args, "--user").unwrap_or_default();
    let limit = flag(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    print_success(
        "analytics positions",
        client.current_positions(&user, limit).await?,
    )
}

async fn data_trades(client: &Client, args: &[String]) -> Result<()> {
    let user = flag(args, "--user").unwrap_or_default();
    let limit = flag(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    print_success("analytics trades", client.trades(&user, limit).await?)
}

async fn data_leaderboard(client: &Client, args: &[String]) -> Result<()> {
    let limit = flag(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    print_success(
        "analytics leaderboard",
        client.trader_leaderboard(limit).await?,
    )
}

fn sim_reset(args: &[String]) -> Result<()> {
    let cash = flag(args, "--cash")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10000.0);
    print_success("sim reset", paper::State::new("USD", cash))
}

fn sim_buy(args: &[String]) -> Result<()> {
    let mut state = paper::State::new("USD", 10000.0);
    let fill = state.buy(paper_order(args))?;
    print_success("sim buy", json!({"fill": fill, "state": state}))
}

fn sim_sell(args: &[String]) -> Result<()> {
    let mut state = paper::State::new("USD", 10000.0);
    let order = paper_order(args);
    state.buy(paper::Order {
        price: order.price,
        size: order.size,
        ..order.clone()
    })?;
    let fill = state.sell(order)?;
    print_success("sim sell", json!({"fill": fill, "state": state}))
}

fn paper_order(args: &[String]) -> paper::Order {
    paper::Order {
        market_id: flag(args, "--market-id").unwrap_or_default(),
        token_id: flag(args, "--token-id").unwrap_or_default(),
        price: flag(args, "--price")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0),
        size: flag(args, "--size")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0),
    }
}

async fn stream_watch(args: &[String]) -> Result<()> {
    let tokens = flag_values(args, "--token-id");
    let limit: usize = flag(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let seconds: u64 = flag(args, "--seconds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let mut config = stream::Config::default();
    if let Some(url) = flag(args, "--url") {
        config.url = url;
    }
    let mut client = stream_client::MarketWsClient::connect_with_retries(config).await?;
    if !tokens.is_empty() {
        client.subscribe_assets(&tokens).await?;
    }
    let deadline = Instant::now() + Duration::from_secs(seconds.max(1));
    let mut events = Vec::new();
    while events.len() < limit && Instant::now() < deadline {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        events.extend(client.read_raw(now_ms).await?);
    }
    let stats = client.stats();
    let _ = tokio::time::timeout(Duration::from_secs(1), client.close()).await;
    print_success("stream watch", json!({"events": events, "stats": stats}))
}

fn flag_values(args: &[String], name: &str) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == name)
        .map(|w| w[1].clone())
        .collect()
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find_map(|w| (w[0] == name).then(|| w[1].clone()))
}

fn print_success<T: serde::Serialize>(command: &str, data: T) -> Result<()> {
    print!("{}", output::success(command, data)?);
    Ok(())
}

fn print_help() {
    println!("polyrover async Polymarket CLI\n\nUsage: polyrover <command> [options]\n\nCommands:\n  Public data:\n    ping                       Check API health\n    gamma search               Search Gamma markets, events, and profiles\n    gamma markets              List Gamma markets\n    clob book                  Fetch an order book\n    clob price                 Fetch a side price\n    clob simulate              Estimate a fill\n    analytics positions        Fetch wallet positions\n    analytics trades           Fetch wallet trades\n    analytics leaderboard      Fetch the trader leaderboard\n\n  Streaming:\n    stream watch               Watch public market events\n\n  Local simulation:\n    sim reset                  Create a fresh paper state\n    sim buy                    Apply a local paper buy\n    sim sell                   Apply a local paper sell\n\nGlobal options:\n  --json        Print the versioned JSON envelope\n  -h, --help    Show help\n\nRun `polyrover help <command>` for command-specific usage and examples.");
}

fn print_command_help(command: &[String]) -> Result<()> {
    if let [group] = command {
        let details = match group.as_str() {
            "gamma" => Some((
                "Query public Gamma discovery APIs.",
                "  search     Search markets, events, and profiles\n  markets    List markets",
            )),
            "clob" => Some((
                "Read public CLOB data and estimate fills.",
                "  book        Fetch an order book\n  price       Fetch a side price\n  simulate    Estimate a fill",
            )),
            "analytics" => Some((
                "Read public wallet and leaderboard data.",
                "  positions      Fetch wallet positions\n  trades         Fetch wallet trades\n  leaderboard    Fetch the trader leaderboard",
            )),
            "stream" => Some((
                "Read public market WebSocket events.",
                "  watch    Watch market events",
            )),
            "sim" => Some((
                "Apply local paper-state operations.",
                "  reset    Create a fresh state\n  buy      Apply a paper buy\n  sell     Apply a paper sell",
            )),
            _ => None,
        };
        if let Some((description, commands)) = details {
            print_group_help(group, description, commands);
            return Ok(());
        }
    }

    let (description, usage, options, example) = match command {
        [command] if command == "ping" => (
            "Check Gamma, CLOB, and Data API health.",
            "ping [--json]",
            "",
            "polyrover ping --json",
        ),
        [group, command] if group == "gamma" && command == "search" => (
            "Search Gamma markets, events, and profiles.",
            "gamma search --query <text> [--limit <n>] [--json]",
            "  --query <text>    Search text (required)\n  --limit <n>       Maximum results per type\n",
            "polyrover gamma search --query \"bitcoin\" --limit 3 --json",
        ),
        [group, command] if group == "gamma" && command == "markets" => (
            "List Gamma markets.",
            "gamma markets [--limit <n>] [--json]",
            "  --limit <n>    Maximum results\n",
            "polyrover gamma markets --limit 3 --json",
        ),
        [group, command] if group == "clob" && command == "book" => (
            "Fetch a token's CLOB order book.",
            "clob book --token-id <id> [--json]",
            "  --token-id <id>    CLOB token ID (required)\n",
            "polyrover clob book --token-id TOKEN_ID --json",
        ),
        [group, command] if group == "clob" && command == "price" => (
            "Fetch a token's CLOB price for one side.",
            "clob price --token-id <id> [--side buy|sell] [--json]",
            "  --token-id <id>    CLOB token ID (required)\n  --side <side>      buy or sell (default: buy)\n",
            "polyrover clob price --token-id TOKEN_ID --side buy --json",
        ),
        [group, command] if group == "clob" && command == "simulate" => (
            "Estimate a fill against the current CLOB book.",
            "clob simulate --token <id> --amount <n> [--side buy|sell] [--limit-price <p>] [--json]",
            "  --token <id>        CLOB token ID (required; --token-id also accepted)\n  --amount <n>       Amount to simulate (required)\n  --side <side>      buy or sell (default: buy)\n  --limit-price <p>  Optional price limit\n",
            "polyrover clob simulate --token TOKEN_ID --amount 100 --limit-price 0.55 --json",
        ),
        [group, command] if group == "analytics" && command == "positions" => (
            "Fetch a wallet's current positions.",
            "analytics positions --user <wallet> [--limit <n>] [--json]",
            "  --user <wallet>    Wallet address (required)\n  --limit <n>        Maximum results (default: 20)\n",
            "polyrover analytics positions --user 0x1234 --limit 10 --json",
        ),
        [group, command] if group == "analytics" && command == "trades" => (
            "Fetch a wallet's trades.",
            "analytics trades --user <wallet> [--limit <n>] [--json]",
            "  --user <wallet>    Wallet address (required)\n  --limit <n>        Maximum results (default: 20)\n",
            "polyrover analytics trades --user 0x1234 --limit 10 --json",
        ),
        [group, command] if group == "analytics" && command == "leaderboard" => (
            "Fetch the trader leaderboard.",
            "analytics leaderboard [--limit <n>] [--json]",
            "  --limit <n>    Maximum results (default: 20)\n",
            "polyrover analytics leaderboard --limit 10 --json",
        ),
        [group, command] if group == "stream" && command == "watch" => (
            "Watch public market WebSocket events.",
            "stream watch [--token-id <id> ...] [--url <ws-url>] [--limit <n>] [--seconds <n>] [--json]",
            "  --token-id <id>    Token to subscribe to; repeat for multiple tokens\n  --url <ws-url>      WebSocket endpoint (default: Polymarket market stream)\n  --limit <n>         Stop after this many events (default: 10)\n  --seconds <n>       Stop after this many seconds (default: 30)\n",
            "polyrover stream watch --token-id TOKEN_ID --limit 10 --seconds 30 --json",
        ),
        [group, command] if group == "sim" && command == "reset" => (
            "Create a fresh local paper state.",
            "sim reset [--cash <n>] [--json]",
            "  --cash <n>    Starting USD cash (default: 10000)\n",
            "polyrover sim reset --cash 5000 --json",
        ),
        [group, command] if group == "sim" && command == "buy" => (
            "Apply a local paper buy.",
            "sim buy --token-id <id> --price <p> [--size <n>] [--market-id <id>] [--json]",
            "  --token-id <id>   Token ID (required)\n  --price <p>        Fill price (required)\n  --size <n>         Fill size (default: 1)\n  --market-id <id>   Optional market ID\n",
            "polyrover sim buy --token-id TOKEN_ID --price 0.55 --size 10 --json",
        ),
        [group, command] if group == "sim" && command == "sell" => (
            "Apply a local paper sell.",
            "sim sell --token-id <id> --price <p> [--size <n>] [--market-id <id>] [--json]",
            "  --token-id <id>   Token ID (required)\n  --price <p>        Fill price (required)\n  --size <n>         Fill size (default: 1)\n  --market-id <id>   Optional market ID\n",
            "polyrover sim sell --token-id TOKEN_ID --price 0.60 --size 10 --json",
        ),
        [] => {
            print_help();
            return Ok(());
        }
        _ => return Err(unknown_command(command)),
    };
    println!(
        "{description}\n\nUsage: polyrover {usage}\n\nOptions:\n{options}  --json        Print the versioned JSON envelope\n  -h, --help    Show this help\n\nExample:\n  {example}"
    );
    Ok(())
}

fn print_group_help(group: &str, description: &str, commands: &str) {
    println!(
        "{description}\n\nUsage: polyrover {group} <command> [options]\n\nCommands:\n{commands}\n\nGlobal options:\n  --json        Print the versioned JSON envelope\n  -h, --help    Show help\n\nRun `polyrover help {group} <command>` for command-specific details."
    );
}

fn unknown_command(command: &[String]) -> Error {
    Error::Invalid(format!(
        "unknown command `{}`; run `polyrover help` to list commands",
        command.join(" ")
    ))
}
