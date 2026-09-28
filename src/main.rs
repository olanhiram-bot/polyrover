//! `polyrover` CLI entrypoint dispatching to the SDK modules.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use polyrover::{
    clob::{BatchPriceHistoryParams, PriceHistoryParams},
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
        [group, cmd, rest @ ..] if group == "gamma" && cmd == "market-page" => {
            gamma_market_page(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "gamma" && cmd == "events" => {
            gamma_events(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "gamma" && cmd == "event-page" => {
            gamma_event_page(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "fees" => clob_fees(rest),
        [group, cmd, rest @ ..] if group == "clob" && cmd == "book" => {
            clob_book(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "price" => {
            clob_price(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "fee-rate" => {
            clob_fee_rate(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "simulate" => {
            clob_simulate(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "price-history" => {
            clob_price_history(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "clob" && cmd == "batch-price-history" => {
            clob_batch_price_history(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "positions" => {
            data_positions(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "trades" => {
            data_trades(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "closed-positions" => {
            data_closed_positions(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "activity" => {
            data_activity(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "leaderboard" => {
            data_leaderboard(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "builder-leaderboard" => {
            data_builder_leaderboard(&client, rest).await
        }
        [group, cmd, rest @ ..] if group == "analytics" && cmd == "builder-volume" => {
            data_builder_volume(&client, rest).await
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
    print_success("gamma markets", client.markets(&market_params(args)).await?)
}

async fn gamma_market_page(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "gamma market-page",
        client.market_page(&market_keyset_params(args)).await?,
    )
}

async fn gamma_events(client: &Client, args: &[String]) -> Result<()> {
    print_success("gamma events", client.events(&event_params(args)).await?)
}

async fn gamma_event_page(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "gamma event-page",
        client.event_page(&event_keyset_params(args)).await?,
    )
}

fn clob_fees(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        return Err(unknown_command(
            &[vec!["clob".into(), "fees".into()], args.to_vec()].concat(),
        ));
    }
    print_success(
        "clob fees",
        json!({
            "formula": "shares * taker_fee_rate * price * (1 - price)",
            "precision_usdc": "0.00001",
            "makers_pay_fees": false,
            "categories": simulation::FEE_SCHEDULE,
            "order_types": [
                {"type": "GTC", "behavior": "rests until filled or cancelled"},
                {"type": "GTD", "behavior": "rests until its expiration"},
                {"type": "FOK", "behavior": "fills entirely or cancels immediately"},
                {"type": "FAK", "behavior": "fills available liquidity and cancels the remainder"}
            ],
            "post_only": "rests as maker or is rejected if it would match immediately",
            "sources": [
                "https://docs.polymarket.com/trading/fees",
                "https://docs.polymarket.com/concepts/order-lifecycle"
            ]
        }),
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

async fn clob_fee_rate(client: &Client, args: &[String]) -> Result<()> {
    let token = flag(args, "--token-id").unwrap_or_default();
    print_success("clob fee-rate", client.fee_rate(&token).await?)
}

async fn clob_simulate(client: &Client, args: &[String]) -> Result<()> {
    let request = simulation::Request {
        token_id: flag(args, "--token")
            .or_else(|| flag(args, "--token-id"))
            .unwrap_or_default(),
        side: flag(args, "--side").unwrap_or_else(|| "buy".into()),
        amount: flag(args, "--amount").unwrap_or_default(),
        limit_price: flag(args, "--limit-price").unwrap_or_default(),
    };
    let category = flag(args, "--fee-category").unwrap_or_default();
    let result = if category.is_empty() {
        client.simulate(request).await?
    } else {
        client.simulate_with_fee(request, &category).await?
    };
    print_success("clob simulate", result)
}

fn clob_history_params(args: &[String]) -> PriceHistoryParams {
    PriceHistoryParams {
        token_id: flag(args, "--token-id").unwrap_or_default(),
        start_ts: flag(args, "--start-ts").and_then(|v| v.parse().ok()),
        end_ts: flag(args, "--end-ts").and_then(|v| v.parse().ok()),
        interval: flag(args, "--interval"),
        fidelity: flag(args, "--fidelity").and_then(|v| v.parse().ok()),
    }
}

fn batch_history_params(args: &[String]) -> BatchPriceHistoryParams {
    BatchPriceHistoryParams {
        markets: flag_values(args, "--token-id"),
        start_ts: flag(args, "--start-ts").and_then(|v| v.parse().ok()),
        end_ts: flag(args, "--end-ts").and_then(|v| v.parse().ok()),
        interval: flag(args, "--interval"),
        fidelity: flag(args, "--fidelity").and_then(|v| v.parse().ok()),
    }
}

async fn clob_price_history(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "clob price-history",
        client.price_history(&clob_history_params(args)).await?,
    )
}

async fn clob_batch_price_history(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "clob batch-price-history",
        client
            .batch_price_history(&batch_history_params(args))
            .await?,
    )
}

fn bool_flag(args: &[String], name: &str) -> Option<bool> {
    flag(args, name).and_then(|value| value.parse().ok())
}

fn integer_values(args: &[String], name: &str) -> Vec<i64> {
    flag_values(args, name)
        .into_iter()
        .filter_map(|value| value.parse().ok())
        .collect()
}

fn market_params(args: &[String]) -> gamma::MarketParams {
    gamma::MarketParams {
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        offset: flag(args, "--offset").and_then(|v| v.parse().ok()),
        order: flag(args, "--order"),
        ascending: bool_flag(args, "--ascending"),
        ids: integer_values(args, "--id"),
        slug: flag_values(args, "--slug"),
        condition_ids: flag_values(args, "--condition-id"),
        clob_token_ids: flag_values(args, "--clob-token-id"),
        market_maker_address: flag(args, "--market-maker-address").unwrap_or_default(),
        active: bool_flag(args, "--active"),
        closed: bool_flag(args, "--closed"),
        tag_id: flag(args, "--tag-id").and_then(|v| v.parse().ok()),
        liquidity_num_min: flag(args, "--liquidity-min").and_then(|v| v.parse().ok()),
        liquidity_num_max: flag(args, "--liquidity-max").and_then(|v| v.parse().ok()),
        volume_num_min: flag(args, "--volume-min").and_then(|v| v.parse().ok()),
        volume_num_max: flag(args, "--volume-max").and_then(|v| v.parse().ok()),
        start_date_min: flag(args, "--start-date-min").unwrap_or_default(),
        start_date_max: flag(args, "--start-date-max").unwrap_or_default(),
        end_date_min: flag(args, "--end-date-min").unwrap_or_default(),
        end_date_max: flag(args, "--end-date-max").unwrap_or_default(),
        related_tags: bool_flag(args, "--related-tags"),
        cyom: bool_flag(args, "--cyom"),
        uma_resolution_status: flag(args, "--uma-resolution-status").unwrap_or_default(),
        game_id: flag(args, "--game-id").unwrap_or_default(),
        rewards_min_size: flag(args, "--rewards-min-size").and_then(|v| v.parse().ok()),
        question_ids: flag_values(args, "--question-id"),
        include_tag: bool_flag(args, "--include-tag"),
        sports_market_types: flag_values(args, "--sports-market-type"),
    }
}

fn market_keyset_params(args: &[String]) -> gamma::MarketKeysetParams {
    gamma::MarketKeysetParams {
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        after_cursor: flag(args, "--after-cursor").unwrap_or_default(),
        order: flag(args, "--order"),
        ascending: bool_flag(args, "--ascending"),
        ids: integer_values(args, "--id"),
        slug: flag_values(args, "--slug"),
        decimalized: bool_flag(args, "--decimalized"),
        condition_ids: flag_values(args, "--condition-id"),
        clob_token_ids: flag_values(args, "--clob-token-id"),
        question_ids: flag_values(args, "--question-id"),
        market_maker_address: flag(args, "--market-maker-address").unwrap_or_default(),
        active: bool_flag(args, "--active"),
        closed: bool_flag(args, "--closed"),
        tag_id: None,
        tag_ids: integer_values(args, "--tag-id"),
        liquidity_num_min: flag(args, "--liquidity-min").and_then(|v| v.parse().ok()),
        liquidity_num_max: flag(args, "--liquidity-max").and_then(|v| v.parse().ok()),
        volume_num_min: flag(args, "--volume-min").and_then(|v| v.parse().ok()),
        volume_num_max: flag(args, "--volume-max").and_then(|v| v.parse().ok()),
        start_date_min: flag(args, "--start-date-min").unwrap_or_default(),
        start_date_max: flag(args, "--start-date-max").unwrap_or_default(),
        end_date_min: flag(args, "--end-date-min").unwrap_or_default(),
        end_date_max: flag(args, "--end-date-max").unwrap_or_default(),
        related_tags: bool_flag(args, "--related-tags"),
        tag_match: flag(args, "--tag-match").unwrap_or_default(),
        cyom: bool_flag(args, "--cyom"),
        rfq_enabled: bool_flag(args, "--rfq-enabled"),
        uma_resolution_status: flag(args, "--uma-resolution-status").unwrap_or_default(),
        game_id: flag(args, "--game-id").unwrap_or_default(),
        include_tag: bool_flag(args, "--include-tag"),
        locale: flag(args, "--locale").unwrap_or_default(),
        sports_market_types: flag_values(args, "--sports-market-type"),
    }
}

fn event_params(args: &[String]) -> gamma::EventParams {
    gamma::EventParams {
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        offset: flag(args, "--offset").and_then(|v| v.parse().ok()),
        order: flag(args, "--order"),
        ascending: bool_flag(args, "--ascending"),
        ids: integer_values(args, "--id"),
        slug: flag_values(args, "--slug"),
        active: bool_flag(args, "--active"),
        closed: bool_flag(args, "--closed"),
        archived: bool_flag(args, "--archived"),
        tag_id: flag(args, "--tag-id").and_then(|v| v.parse().ok()),
        exclude_tag_ids: integer_values(args, "--exclude-tag-id"),
        tag_slug: flag(args, "--tag-slug").unwrap_or_default(),
        related_tags: bool_flag(args, "--related-tags"),
        featured: bool_flag(args, "--featured"),
        cyom: bool_flag(args, "--cyom"),
        include_chat: bool_flag(args, "--include-chat"),
        include_template: bool_flag(args, "--include-template"),
        recurrence: flag(args, "--recurrence").unwrap_or_default(),
        liquidity_min: flag(args, "--liquidity-min").and_then(|v| v.parse().ok()),
        liquidity_max: flag(args, "--liquidity-max").and_then(|v| v.parse().ok()),
        volume_min: flag(args, "--volume-min").and_then(|v| v.parse().ok()),
        volume_max: flag(args, "--volume-max").and_then(|v| v.parse().ok()),
        start_date_min: flag(args, "--start-date-min").unwrap_or_default(),
        start_date_max: flag(args, "--start-date-max").unwrap_or_default(),
        end_date_min: flag(args, "--end-date-min").unwrap_or_default(),
        end_date_max: flag(args, "--end-date-max").unwrap_or_default(),
    }
}

fn event_keyset_params(args: &[String]) -> gamma::EventKeysetParams {
    gamma::EventKeysetParams {
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        after_cursor: flag(args, "--after-cursor").unwrap_or_default(),
        order: flag(args, "--order"),
        ascending: bool_flag(args, "--ascending"),
        ids: integer_values(args, "--id"),
        slug: flag_values(args, "--slug"),
        closed: bool_flag(args, "--closed"),
        live: bool_flag(args, "--live"),
        featured: bool_flag(args, "--featured"),
        cyom: bool_flag(args, "--cyom"),
        title_search: flag(args, "--title-search").unwrap_or_default(),
        liquidity_min: flag(args, "--liquidity-min").and_then(|v| v.parse().ok()),
        liquidity_max: flag(args, "--liquidity-max").and_then(|v| v.parse().ok()),
        volume_min: flag(args, "--volume-min").and_then(|v| v.parse().ok()),
        volume_max: flag(args, "--volume-max").and_then(|v| v.parse().ok()),
        start_date_min: flag(args, "--start-date-min").unwrap_or_default(),
        start_date_max: flag(args, "--start-date-max").unwrap_or_default(),
        end_date_min: flag(args, "--end-date-min").unwrap_or_default(),
        end_date_max: flag(args, "--end-date-max").unwrap_or_default(),
        start_time_min: flag(args, "--start-time-min").unwrap_or_default(),
        start_time_max: flag(args, "--start-time-max").unwrap_or_default(),
        tag_ids: integer_values(args, "--tag-id"),
        tag_slug: flag(args, "--tag-slug").unwrap_or_default(),
        exclude_tag_ids: integer_values(args, "--exclude-tag-id"),
        related_tags: bool_flag(args, "--related-tags"),
        tag_match: flag(args, "--tag-match").unwrap_or_default(),
        series_ids: integer_values(args, "--series-id"),
        game_ids: integer_values(args, "--game-id"),
        event_date: flag(args, "--event-date").unwrap_or_default(),
        event_week: flag(args, "--event-week").and_then(|v| v.parse().ok()),
        featured_order: bool_flag(args, "--featured-order"),
        recurrence: flag(args, "--recurrence").unwrap_or_default(),
        created_by: flag_values(args, "--created-by"),
        parent_event_id: flag(args, "--parent-event-id").and_then(|v| v.parse().ok()),
        include_children: bool_flag(args, "--include-children"),
        partner_slug: flag(args, "--partner-slug").unwrap_or_default(),
        include_chat: bool_flag(args, "--include-chat"),
        include_template: bool_flag(args, "--include-template"),
        include_best_lines: bool_flag(args, "--include-best-lines"),
        locale: flag(args, "--locale").unwrap_or_default(),
    }
}

fn trade_params(args: &[String]) -> polyrover::data::TradeParams {
    polyrover::data::TradeParams {
        user: flag(args, "--user").unwrap_or_default(),
        markets: flag_values(args, "--market"),
        event_ids: flag_values(args, "--event-id")
            .into_iter()
            .filter_map(|v| v.parse().ok())
            .collect(),
        side: flag(args, "--side").unwrap_or_default(),
        start: flag(args, "--start").and_then(|v| v.parse().ok()),
        end: flag(args, "--end").and_then(|v| v.parse().ok()),
        taker_only: bool_flag(args, "--taker-only"),
        filter_type: flag(args, "--filter-type").unwrap_or_default(),
        filter_amount: flag(args, "--filter-amount").unwrap_or_default(),
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        offset: flag(args, "--offset").and_then(|v| v.parse().ok()),
    }
}

fn closed_position_params(args: &[String]) -> polyrover::data::ClosedPositionParams {
    polyrover::data::ClosedPositionParams {
        user: flag(args, "--user").unwrap_or_default(),
        markets: flag_values(args, "--market"),
        title: flag(args, "--title").unwrap_or_default(),
        event_ids: flag_values(args, "--event-id")
            .into_iter()
            .filter_map(|v| v.parse().ok())
            .collect(),
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        offset: flag(args, "--offset").and_then(|v| v.parse().ok()),
        sort_by: flag(args, "--sort-by").unwrap_or_default(),
        sort_direction: flag(args, "--sort-direction").unwrap_or_default(),
    }
}

fn activity_params(args: &[String]) -> polyrover::data::ActivityParams {
    polyrover::data::ActivityParams {
        user: flag(args, "--user").unwrap_or_default(),
        markets: flag_values(args, "--market"),
        event_ids: flag_values(args, "--event-id")
            .into_iter()
            .filter_map(|v| v.parse().ok())
            .collect(),
        activity_types: flag_values(args, "--type"),
        side: flag(args, "--side").unwrap_or_default(),
        start: flag(args, "--start").and_then(|v| v.parse().ok()),
        end: flag(args, "--end").and_then(|v| v.parse().ok()),
        sort_by: flag(args, "--sort-by").unwrap_or_default(),
        sort_direction: flag(args, "--sort-direction").unwrap_or_default(),
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        offset: flag(args, "--offset").and_then(|v| v.parse().ok()),
    }
}

fn builder_leaderboard_params(args: &[String]) -> polyrover::data::BuilderLeaderboardParams {
    polyrover::data::BuilderLeaderboardParams {
        time_period: flag(args, "--time-period").unwrap_or_default(),
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        offset: flag(args, "--offset").and_then(|v| v.parse().ok()),
    }
}

fn leaderboard_params(args: &[String]) -> polyrover::data::LeaderboardParams {
    polyrover::data::LeaderboardParams {
        category: flag(args, "--category").unwrap_or_default(),
        time_period: flag(args, "--time-period").unwrap_or_default(),
        order_by: flag(args, "--order-by").unwrap_or_default(),
        limit: flag(args, "--limit").and_then(|v| v.parse().ok()),
        offset: flag(args, "--offset").and_then(|v| v.parse().ok()),
        user: flag(args, "--user").unwrap_or_default(),
        user_name: flag(args, "--user-name").unwrap_or_default(),
    }
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
    print_success(
        "analytics trades",
        client.trades_with(&trade_params(args)).await?,
    )
}

async fn data_closed_positions(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "analytics closed-positions",
        client
            .closed_positions_with(&closed_position_params(args))
            .await?,
    )
}

async fn data_activity(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "analytics activity",
        client.activity_with(&activity_params(args)).await?,
    )
}

async fn data_leaderboard(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "analytics leaderboard",
        client
            .trader_leaderboard_with(&leaderboard_params(args))
            .await?,
    )
}

async fn data_builder_leaderboard(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "analytics builder-leaderboard",
        client
            .builder_leaderboard(&builder_leaderboard_params(args))
            .await?,
    )
}

async fn data_builder_volume(client: &Client, args: &[String]) -> Result<()> {
    print_success(
        "analytics builder-volume",
        client
            .builder_volume(&polyrover::data::BuilderVolumeParams {
                time_period: flag(args, "--time-period").unwrap_or_default(),
            })
            .await?,
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
    println!("polyrover async Polymarket CLI\n\nUsage: polyrover <command> [options]\n\nCommands:\n  Public data:\n    ping                       Check API health\n    gamma search               Search Gamma markets, events, and profiles\n    gamma markets              List Gamma markets\n    gamma market-page          Fetch one keyset-paginated market page\n    gamma events               List Gamma events\n    gamma event-page           Fetch one keyset-paginated event page\n    clob book                  Fetch an order book\n    clob price                 Fetch a side price\n    clob fee-rate              Fetch a token's base fee in bps\n    clob fees                  Show order types and the documented fee schedule\n    clob simulate              Estimate a fill, optionally including taker fees\n    clob price-history         Fetch one token's historical price series\n    clob batch-price-history   Fetch up to 20 historical price series\n    analytics positions        Fetch wallet positions\n    analytics trades           Fetch trades\n    analytics closed-positions Fetch wallet closed positions\n    analytics activity         Fetch wallet activity\n    analytics leaderboard      Fetch the trader leaderboard\n    analytics builder-leaderboard Fetch the aggregated builder leaderboard\n    analytics builder-volume   Fetch daily builder volume history\n\n  Streaming:\n    stream watch               Watch public market events\n\n  Local simulation:\n    sim reset                  Create a fresh paper state\n    sim buy                    Apply a local paper buy\n    sim sell                   Apply a local paper sell\n\nGlobal options:\n  --json        Print the versioned JSON envelope\n  -h, --help    Show help\n\nRun `polyrover help <command>` for command-specific usage and examples.\nOfficial API guide: https://docs.polymarket.com/getting-started/api");
}

fn print_command_help(command: &[String]) -> Result<()> {
    if let [group] = command {
        let details = match group.as_str() {
            "gamma" => Some((
                "Query public Gamma discovery APIs. Historical commands make one bounded upstream request. Callers own pagination.",
                "  search         Search markets, events, and profiles\n  markets        List offset-paginated markets\n  market-page    Fetch one keyset-paginated market page\n  events         List offset-paginated events\n  event-page     Fetch one keyset-paginated event page",
            )),
            "clob" => Some((
                "Read public CLOB data, inspect fees and order types, and estimate fills. Historical commands make one bounded upstream request. Callers own pagination.",
                "  book        Fetch an order book\n  price       Fetch a side price\n  fee-rate    Fetch a token's base fee in bps\n  fees                   Show the documented fee schedule and order types\n  simulate               Estimate a fill, optionally including taker fees\n  price-history          Fetch one token's historical price series\n  batch-price-history    Fetch up to 20 historical price series",
            )),
            "analytics" => Some((
                "Read public wallet and leaderboard data. Historical commands make one bounded upstream request. Callers own pagination.",
                "  positions           Fetch wallet positions\n  trades              Fetch trades\n  closed-positions    Fetch wallet closed positions\n  activity            Fetch wallet activity\n  leaderboard         Fetch the trader leaderboard\n  builder-leaderboard Fetch the aggregated builder leaderboard\n  builder-volume      Fetch daily builder volume history",
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
            "Fetch one offset-paginated Gamma market request.",
            "gamma markets [--limit <n>] [--offset <n>] [--active <bool>] [--closed <bool>] [--start-date-min <iso>] [--start-date-max <iso>] [--end-date-min <iso>] [--end-date-max <iso>] [--json]",
            "  --limit/--offset <n>             Page controls\n  --order <field> --ascending <bool>  Sort controls\n  --id/--slug/--condition-id/--clob-token-id/--question-id <id>  Repeatable identifiers\n  --market-maker-address <address>    Maker filter\n  --active/--closed/--cyom <bool>     Status filters\n  --tag-id <id>                       Tag filter\n  --liquidity-min/--liquidity-max <n>  Liquidity bounds\n  --volume-min/--volume-max <n>       Volume bounds\n  --start-date-min/--start-date-max <iso>  Start bounds\n  --end-date-min/--end-date-max <iso>  End bounds\n  --uma-resolution-status <status>    UMA status filter\n  --game-id <id>                      Game filter\n  --rewards-min-size <n>              Reward-size floor\n  --related-tags/--include-tag <bool>  Tag expansion\n  --sports-market-type <type>         Repeatable sports type\n",
            "polyrover gamma markets --closed true --limit 100 --offset 0 --json",
        ),
        [group, command] if group == "gamma" && command == "market-page" => (
            "Fetch one keyset-paginated Gamma market request; copy next_cursor unchanged.",
            "gamma market-page [--limit <n>] [--after-cursor <cursor>] [--active <bool>] [--closed <bool>] [--start-date-min <iso>] [--start-date-max <iso>] [--end-date-min <iso>] [--end-date-max <iso>] [--json]",
            "  --limit <n>                        Maximum 1000\n  --after-cursor <cursor>             Opaque prior next_cursor\n  --order <field> --ascending <bool>  Sort controls\n  --id/--slug/--condition-id/--clob-token-id/--question-id <id>  Repeatable identifiers\n  --market-maker-address <address>    Maker filter\n  --active/--closed/--decimalized <bool>  Market filters\n  --tag-id <id> --tag-match <mode>    Tag filters\n  --liquidity-min/--liquidity-max <n>  Liquidity bounds\n  --volume-min/--volume-max <n>       Volume bounds\n  --start-date-min/--start-date-max <iso>  Start bounds\n  --end-date-min/--end-date-max <iso>  End bounds\n  --cyom/--rfq-enabled <bool>         Type filters\n  --uma-resolution-status <status>    UMA status filter\n  --game-id <id> --locale <locale>    Game and locale filters\n  --related-tags/--include-tag <bool>  Tag expansion\n  --sports-market-type <type>         Repeatable sports type\n",
            "polyrover gamma market-page --closed true --limit 100 --after-cursor CURSOR --json",
        ),
        [group, command] if group == "gamma" && command == "events" => (
            "Fetch one offset-paginated Gamma event request.",
            "gamma events [--limit <n>] [--offset <n>] [--closed <bool>] [--archived <bool>] [--start-date-min <iso>] [--start-date-max <iso>] [--end-date-min <iso>] [--end-date-max <iso>] [--json]",
            "  --limit/--offset <n>             Page controls\n  --order <field> --ascending <bool>  Sort controls\n  --id <id> --slug <slug>             Repeatable identifiers\n  --active/--closed/--archived/--featured/--cyom <bool>  Status filters\n  --tag-id <id> --exclude-tag-id <id> --tag-slug <slug>  Tag filters\n  --related-tags/--include-chat/--include-template <bool>  Expansions\n  --recurrence <value>                Recurrence filter\n  --liquidity-min/--liquidity-max <n>  Liquidity bounds\n  --volume-min/--volume-max <n>       Volume bounds\n  --start-date-min/--start-date-max <iso>  Start bounds\n  --end-date-min/--end-date-max <iso>  End bounds\n",
            "polyrover gamma events --closed true --limit 100 --offset 0 --json",
        ),
        [group, command] if group == "gamma" && command == "event-page" => (
            "Fetch one keyset-paginated Gamma event request; copy next_cursor unchanged.",
            "gamma event-page [--limit <n>] [--after-cursor <cursor>] [--closed <bool>] [--live <bool>] [--start-date-min <iso>] [--start-date-max <iso>] [--end-date-min <iso>] [--end-date-max <iso>] [--json]",
            "  --limit <n>                        Maximum 500\n  --after-cursor <cursor>             Opaque prior next_cursor\n  --order <field> --ascending <bool>  Sort controls\n  --id <id> --slug <slug>             Repeatable identifiers\n  --closed/--live/--featured/--cyom <bool>  Status filters\n  --title-search <text>               Title filter\n  --liquidity-min/--liquidity-max <n>  Liquidity bounds\n  --volume-min/--volume-max <n>       Volume bounds\n  --start-date-min/--start-date-max <iso>  Start-date bounds\n  --end-date-min/--end-date-max <iso>  End-date bounds\n  --start-time-min/--start-time-max <iso>  Start-time bounds\n  --tag-id/--exclude-tag-id/--series-id/--game-id <id>  Repeatable IDs\n  --tag-slug/--tag-match <value>      Tag filters\n  --related-tags <bool>               Related-tag expansion\n  --event-date <iso> --event-week <n> Event filters\n  --featured-order <bool> --recurrence <value>  Ordering/recurrence\n  --created-by <id> --parent-event-id <id> --include-children <bool>  Hierarchy\n  --partner-slug <slug> --locale <locale>  Partner/locale filters\n  --include-chat/--include-template/--include-best-lines <bool>  Expansions\n",
            "polyrover gamma event-page --closed true --limit 100 --after-cursor CURSOR --json",
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
        [group, command] if group == "clob" && command == "fee-rate" => (
            "Fetch a token's CLOB base fee in basis points.",
            "clob fee-rate --token-id <id> [--json]",
            "  --token-id <id>    CLOB token ID (required)\n",
            "polyrover clob fee-rate --token-id TOKEN_ID --json",
        ),
        [group, command] if group == "clob" && command == "fees" => (
            "Show Polymarket order types and the documented category fee schedule. Makers pay no trading fee; takers pay shares × rate × price × (1 - price).",
            "clob fees [--json]",
            "",
            "polyrover clob fees --json",
        ),
        [group, command] if group == "clob" && command == "simulate" => (
            "Estimate a taker fill against the current CLOB book. Makers pay no trading fee. Add --fee-category to estimate the documented taker fee per consumed level.",
            "clob simulate --token <id> --amount <n> [--side buy|sell] [--limit-price <p>] [--fee-category <category>] [--json]",
            "  --token <id>          CLOB token ID (required; --token-id also accepted)\n  --amount <n>         Amount to simulate (required)\n  --side <side>        buy or sell (default: buy)\n  --limit-price <p>    Optional price limit\n  --fee-category <c>  crypto|sports|finance|politics|economics|culture|weather|other|mentions|tech|geopolitics\n",
            "polyrover clob simulate --token TOKEN_ID --amount 100 --fee-category crypto --json",
        ),
        [group, command] if group == "clob" && command == "price-history" => (
            "Fetch one bounded historical price request.",
            "clob price-history --token-id <id> [--start-ts <unix>] [--end-ts <unix>] [--interval max|all|1m|1w|1d|6h|1h] [--fidelity <minutes>] [--json]",
            "  --token-id <id>       CLOB token ID (required)\n  --start-ts <unix>     Inclusive start time\n  --end-ts <unix>       Inclusive end time\n  --interval <value>    max, all, 1m, 1w, 1d, 6h, or 1h\n  --fidelity <minutes>  Sampling fidelity\n",
            "polyrover clob price-history --token-id TOKEN_ID --interval 1d --fidelity 5 --json",
        ),
        [group, command] if group == "clob" && command == "batch-price-history" => (
            "Fetch one bounded historical price request for at most 20 asset IDs.",
            "clob batch-price-history --token-id <id>... [--start-ts <unix>] [--end-ts <unix>] [--interval max|all|1m|1w|1d|6h|1h] [--fidelity <minutes>] [--json]",
            "  --token-id <id>       CLOB token ID; repeat 1 to 20 times\n  --start-ts <unix>     Inclusive start time\n  --end-ts <unix>       Inclusive end time\n  --interval <value>    max, all, 1m, 1w, 1d, 6h, or 1h\n  --fidelity <minutes>  Sampling fidelity\n",
            "polyrover clob batch-price-history --token-id TOKEN_1 --token-id TOKEN_2 --interval 1d --json",
        ),
        [group, command] if group == "analytics" && command == "positions" => (
            "Fetch a wallet's current positions.",
            "analytics positions --user <wallet> [--limit <n>] [--json]",
            "  --user <wallet>    Wallet address (required)\n  --limit <n>        Maximum results (default: 20)\n",
            "polyrover analytics positions --user 0x1234 --limit 10 --json",
        ),
        [group, command] if group == "analytics" && command == "trades" => (
            "Fetch one bounded trade request. Omit --start for the recent default window; --start 1 extends only user-scoped available history.",
            "analytics trades [--user <wallet>] [--market <condition>...] [--event-id <id>...] [--side BUY|SELL] [--start <unix>] [--end <unix>] [--taker-only <bool>] [--filter-type CASH|TOKENS] [--filter-amount <n>] [--limit <n>] [--offset <n>] [--json]",
            "  --user <wallet>         Public wallet filter\n  --market <condition>    Repeatable market filter\n  --event-id <id>         Repeatable event filter\n  --side <side>           BUY or SELL\n  --start/--end <unix>    Time window\n  --taker-only <bool>     Taker filter\n  --filter-type <type>    CASH or TOKENS\n  --filter-amount <n>     Amount threshold\n  --limit/--offset <n>    One page, maximum 10000 each\n",
            "polyrover analytics trades --user 0x1234 --start 1 --limit 100 --offset 0 --json",
        ),
        [group, command] if group == "analytics" && command == "closed-positions" => (
            "Fetch one bounded closed-position request.",
            "analytics closed-positions --user <wallet> [--market <condition>...] [--event-id <id>...] [--title <text>] [--sort-by <field>] [--sort-direction ASC|DESC] [--limit <n>] [--offset <n>] [--json]",
            "  --user <wallet>         Wallet address (required)\n  --market <condition>    Repeatable market filter\n  --event-id <id>         Repeatable event filter\n  --title <text>          Title filter\n  --sort-by <field>       Sort field\n  --sort-direction <dir>  ASC or DESC\n  --limit/--offset <n>    Page controls\n",
            "polyrover analytics closed-positions --user 0x1234 --limit 100 --offset 0 --json",
        ),
        [group, command] if group == "analytics" && command == "activity" => (
            "Fetch one bounded activity request; offsets above 5000 require caller-managed time windows.",
            "analytics activity --user <wallet> [--market <condition>...] [--event-id <id>...] [--type <type>...] [--side BUY|SELL] [--start <unix>] [--end <unix>] [--sort-by <field>] [--sort-direction ASC|DESC] [--limit <n>] [--offset <n>] [--json]",
            "  --user <wallet>         Wallet address (required)\n  --market <condition>    Repeatable market filter\n  --event-id <id>         Repeatable event filter\n  --type <type>           Repeatable activity type\n  --side <side>           BUY or SELL\n  --start/--end <unix>    Time window\n  --sort-by <field>       Sort field\n  --sort-direction <dir>  ASC or DESC\n  --limit <n>             Maximum 500\n  --offset <n>            Maximum 5000\n",
            "polyrover analytics activity --user 0x1234 --start 1 --end 100 --limit 100 --json",
        ),
        [group, command] if group == "analytics" && command == "leaderboard" => (
            "Fetch one bounded trader-leaderboard request.",
            "analytics leaderboard [--category <category>] [--time-period DAY|WEEK|MONTH|ALL] [--order-by <field>] [--user <wallet>] [--user-name <name>] [--limit <n>] [--offset <n>] [--json]",
            "  --category <category>    Category filter\n  --time-period <period>  DAY, WEEK, MONTH, or ALL\n  --order-by <field>      Sort field\n  --user <wallet>         Wallet filter\n  --user-name <name>      Username filter\n  --limit/--offset <n>    Page controls\n",
            "polyrover analytics leaderboard --time-period MONTH --limit 100 --offset 0 --json",
        ),
        [group, command] if group == "analytics" && command == "builder-leaderboard" => (
            "Fetch one bounded aggregated builder-leaderboard request.",
            "analytics builder-leaderboard [--time-period DAY|WEEK|MONTH|ALL] [--limit <n>] [--offset <n>] [--json]",
            "  --time-period <period>  DAY, WEEK, MONTH, or ALL\n  --limit <n>             Maximum 50\n  --offset <n>            Maximum 1000\n",
            "polyrover analytics builder-leaderboard --time-period MONTH --limit 25 --offset 0 --json",
        ),
        [group, command] if group == "analytics" && command == "builder-volume" => (
            "Fetch one daily builder-volume time-series request.",
            "analytics builder-volume [--time-period DAY|WEEK|MONTH|ALL] [--json]",
            "  --time-period <period>  DAY, WEEK, MONTH, or ALL\n",
            "polyrover analytics builder-volume --time-period ALL --json",
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
        "{description}\n\nUsage: polyrover {usage}\n\nOptions:\n{options}  --json        Print the versioned JSON envelope\n  -h, --help    Show this help\n\nExample:\n  {example}\n\nOfficial API guide: https://docs.polymarket.com/getting-started/api"
    );
    Ok(())
}

fn print_group_help(group: &str, description: &str, commands: &str) {
    println!(
        "{description}\n\nUsage: polyrover {group} <command> [options]\n\nCommands:\n{commands}\n\nGlobal options:\n  --json        Print the versioned JSON envelope\n  -h, --help    Show help\n\nRun `polyrover help {group} <command>` for command-specific details.\nOfficial API guide: https://docs.polymarket.com/getting-started/api"
    );
}

fn unknown_command(command: &[String]) -> Error {
    Error::Invalid(format!(
        "unknown command `{}`; run `polyrover help` to list commands",
        command.join(" ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clob_history_flags_build_one_atomic_request() {
        let args = vec![
            "--token-id".into(),
            "token-1".into(),
            "--start-ts".into(),
            "1".into(),
            "--end-ts".into(),
            "100".into(),
            "--interval".into(),
            "1d".into(),
            "--fidelity".into(),
            "5".into(),
        ];
        let params = clob_history_params(&args);
        assert_eq!(params.token_id, "token-1");
        assert_eq!(params.start_ts, Some(1));
        assert_eq!(params.end_ts, Some(100));
        assert_eq!(params.interval.as_deref(), Some("1d"));
        assert_eq!(params.fidelity, Some(5));

        let batch = vec![
            "--token-id".into(),
            "token-1".into(),
            "--token-id".into(),
            "token-2".into(),
        ];
        assert_eq!(batch_history_params(&batch).markets.len(), 2);
    }

    #[test]
    fn historical_cli_builders_preserve_windows_offsets_and_cursors() {
        let args = vec![
            "--user".into(),
            "0xabc".into(),
            "--start".into(),
            "1".into(),
            "--end".into(),
            "100".into(),
            "--offset".into(),
            "200".into(),
        ];
        let trades = trade_params(&args);
        assert_eq!(trades.user, "0xabc");
        assert_eq!(trades.start, Some(1));
        assert_eq!(trades.end, Some(100));
        assert_eq!(trades.offset, Some(200));

        let event_args = vec![
            "--after-cursor".into(),
            "opaque==".into(),
            "--closed".into(),
            "true".into(),
            "--start-date-min".into(),
            "2026-01-01T00:00:00Z".into(),
        ];
        let events = event_keyset_params(&event_args);
        assert_eq!(events.after_cursor, "opaque==");
        assert_eq!(events.closed, Some(true));
        assert_eq!(events.start_date_min, "2026-01-01T00:00:00Z");

        let builders = builder_leaderboard_params(&[
            "--time-period".into(),
            "MONTH".into(),
            "--limit".into(),
            "25".into(),
            "--offset".into(),
            "50".into(),
        ]);
        assert_eq!(builders.time_period, "MONTH");
        assert_eq!(builders.limit, Some(25));
        assert_eq!(builders.offset, Some(50));
    }
}
