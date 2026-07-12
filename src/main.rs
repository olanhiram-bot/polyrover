use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use polyrover::{
    gamma, output, paper, simulation, stream, stream_client, Client, ClientConfig, Result,
};
use serde_json::json;

fn main() {
    if let Err(err) = run() {
        let body = output::error("polyrover", "error", &err.to_string())
            .unwrap_or_else(|_| format!("error: {err}\n"));
        eprint!("{body}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).filter(|a| a != "--json").collect();
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
        [cmd] if cmd == "ping" => ping(&client),
        [group, cmd, rest @ ..] if group == "gamma" && cmd == "search" => {
            gamma_search(&client, rest)
        }
        [group, cmd, rest @ ..] if group == "gamma" && cmd == "markets" => {
            gamma_markets(&client, rest)
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "book" => clob_book(&client, rest),
        [group, cmd, rest @ ..] if group == "clob" && cmd == "price" => clob_price(&client, rest),
        [group, cmd, rest @ ..] if group == "clob" && cmd == "simulate" => {
            clob_simulate(&client, rest)
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "positions" => {
            data_positions(&client, rest)
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "trades" => {
            data_trades(&client, rest)
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "leaderboard" => {
            data_leaderboard(&client, rest)
        }
        [group, cmd, rest @ ..] if group == "stream" && cmd == "watch" => stream_watch(rest),
        [group, cmd, rest @ ..] if group == "sim" && cmd == "reset" => sim_reset(rest),
        [group, cmd, rest @ ..] if group == "sim" && cmd == "buy" => sim_buy(rest),
        [group, cmd, rest @ ..] if group == "sim" && cmd == "sell" => sim_sell(rest),
        _ => {
            print_help();
            Ok(())
        }
    }
}

fn ping(client: &Client) -> Result<()> {
    print_success("ping", client.health())
}

fn gamma_search(client: &Client, args: &[String]) -> Result<()> {
    let query = flag(args, "--query").unwrap_or_default();
    let limit = flag(args, "--limit").and_then(|v| v.parse().ok());
    print_success(
        "gamma search",
        client.search(&gamma::SearchParams {
            q: query,
            limit_per_type: limit,
            ..Default::default()
        })?,
    )
}

fn gamma_markets(client: &Client, args: &[String]) -> Result<()> {
    let limit = flag(args, "--limit").and_then(|v| v.parse().ok());
    print_success(
        "gamma markets",
        client.markets(&gamma::MarketParams {
            limit,
            ..Default::default()
        })?,
    )
}

fn clob_book(client: &Client, args: &[String]) -> Result<()> {
    let token = flag(args, "--token-id").unwrap_or_default();
    print_success("clob book", client.order_book(&token)?)
}

fn clob_price(client: &Client, args: &[String]) -> Result<()> {
    let token = flag(args, "--token-id").unwrap_or_default();
    let side = flag(args, "--side").unwrap_or_else(|| "buy".into());
    print_success("clob price", json!({"price": client.price(&token, &side)?}))
}

fn clob_simulate(client: &Client, args: &[String]) -> Result<()> {
    let token = flag(args, "--token")
        .or_else(|| flag(args, "--token-id"))
        .unwrap_or_default();
    let side = flag(args, "--side").unwrap_or_else(|| "buy".into());
    let amount = flag(args, "--amount").unwrap_or_default();
    let limit_price = flag(args, "--limit-price").unwrap_or_default();
    print_success(
        "clob simulate",
        client.simulate(simulation::Request {
            token_id: token,
            side,
            amount,
            limit_price,
        })?,
    )
}

fn data_positions(client: &Client, args: &[String]) -> Result<()> {
    let user = flag(args, "--user").unwrap_or_default();
    let limit = flag(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    print_success(
        "analytics positions",
        client.current_positions(&user, limit)?,
    )
}

fn data_trades(client: &Client, args: &[String]) -> Result<()> {
    let user = flag(args, "--user").unwrap_or_default();
    let limit = flag(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    print_success("analytics trades", client.trades(&user, limit)?)
}

fn data_leaderboard(client: &Client, args: &[String]) -> Result<()> {
    let limit = flag(args, "--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    print_success("analytics leaderboard", client.trader_leaderboard(limit)?)
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

fn stream_watch(args: &[String]) -> Result<()> {
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
    let mut client = stream_client::MarketWsClient::connect_with_retries(config)?;
    if !tokens.is_empty() {
        client.subscribe_assets(&tokens)?;
    }
    let deadline = Instant::now() + Duration::from_secs(seconds.max(1));
    let mut events = Vec::new();
    while events.len() < limit && Instant::now() < deadline {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or_default();
        events.extend(client.read_raw(now_ms)?);
    }
    let stats = client.stats();
    client.close()?;
    print_success("stream watch", json!({"events": events, "stats": stats}))
}

fn flag_values(args: &[String], name: &str) -> Vec<String> {
    args.windows(2)
        .filter_map(|w| (w[0] == name).then(|| w[1].clone()))
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
    println!("polyrover read-only Polymarket CLI\n\nCommands:\n  ping --json\n  gamma search --query <text> [--limit n] --json\n  gamma markets [--limit n] --json\n  clob book --token-id <id> --json\n  clob price --token-id <id> --side buy|sell --json\n  clob simulate --token <id> --side buy|sell --amount <n> [--limit-price p] --json\n  analytics positions --user <wallet> [--limit n] --json\n  analytics trades --user <wallet> [--limit n] --json\n  analytics leaderboard [--limit n] --json\n  stream watch --token-id <id> [--token-id <id> ...] [--url ws://...] [--limit n] [--seconds s] --json\n  sim reset [--cash n] --json\n  sim buy --token-id <id> --price <p> --size <n> --json\n  sim sell --token-id <id> --price <p> --size <n> --json");
}
