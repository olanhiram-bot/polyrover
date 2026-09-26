use std::collections::BTreeMap;

use chrono::Utc;
use polyrover::{
    decision::{self, Forecast, Policy},
    news, news_research,
    types::Market,
    typesafe, Client, Error, Result,
};
use serde_json::json;

pub async fn run(client: &Client, args: &[String]) -> Result<()> {
    super::print_success("ai decide-market", evaluate(client, args).await?)
}

pub async fn evaluate(client: &Client, args: &[String]) -> Result<serde_json::Value> {
    let options = Options::parse(args)?;
    // Reserve before paid calls. Existing reports are never overwritten.
    let mut output = options
        .output
        .as_ref()
        .map(|path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
        })
        .transpose()
        .map_err(|e| Error::Invalid(format!("cannot create decision report: {e}")))?;
    let market = match &options.slug {
        Some(slug) => Some(client.market_by_slug(slug).await?),
        None => None,
    };
    if let Some(market) = &market {
        decision::binary_tokens(market)?;
    }
    let search = if let Some(url) = &options.news_url {
        let search = news::Search::from_url(url)?;
        if market
            .as_ref()
            .is_some_and(|m| m.question.trim() != search.query.trim())
        {
            return Err(Error::Invalid(
                "news query must match the market question exactly".into(),
            ));
        }
        search
    } else {
        news::Search::new(
            market
                .as_ref()
                .map(|m| m.question.clone())
                .or(options.question.clone())
                .unwrap(),
        )?
    };
    // Do not spend provider quota researching an already closed/expired market.
    if let Some(market) = &market {
        if let Some(reason) = decision::market_ineligible_reason(market, Utc::now()) {
            let report = decision::ineligible_report(market, &options.policy, reason);
            if let Some(file) = &mut output {
                use std::io::Write;
                file.write_all(polyrover::output::success("ai decide-market", &report)?.as_bytes())
                    .map_err(|e| Error::Invalid(format!("cannot save decision report: {e}")))?;
            }
            return Ok(report);
        }
    }
    let evaluator = super::typesafe_cli::evaluator(typesafe::Config {
        model: options.model.clone(),
        ..Default::default()
    })?;
    eprintln!(
        "Investigando todas las noticias de la búsqueda; los artículos bloqueados se registrarán."
    );
    let (collection, supplemental_searches) =
        news::Client::collect_for_forecast(search, options.max_age).await?;
    eprintln!(
        "{} resultados encontrados. Evaluando todo el texto extraído con el evaluador local…",
        collection.articles.len()
    );
    let research = news_research::research_for_market(
        collection,
        Some(&evaluator),
        &options.model,
        0.8,
        options.max_age,
        market.as_ref(),
    )
    .await?;
    eprintln!(
        "Evaluación terminada: {} artículos completos, {} inaccesibles. Preparando pronóstico…",
        research.coverage.evaluated, research.coverage.unavailable
    );
    let selection = decision::forecast_request(&research, market.as_ref(), &options.model)?;
    let forecast = match decision::synthesize(&selection, &evaluator).await {
        Ok(forecast) => forecast,
        Err(error) => Forecast::unavailable(error.to_string()),
    };
    let mut blockers = decision::evidence_blockers(&research, &selection, &options.policy);
    let mut quote_errors = BTreeMap::new();
    let (mut yes_quote, mut no_quote, mut rules_review) = (None, None, None);
    // Fetch quotes LAST so minutes spent reading articles cannot make them stale.
    let current_market: Option<Market> = if let Some(market) = &market {
        match evaluator.review_market(market, 0.8).await {
            Ok(review) => {
                if review.route != typesafe::ReviewRoute::ResearchReady {
                    blockers.push("resolution_rules_need_review".into());
                }
                rules_review = Some(review);
            }
            Err(error) => {
                blockers.push("resolution_review_failed".into());
                quote_errors.insert("rules".to_string(), error.to_string());
            }
        }
        eprintln!("Actualizando mercado, precios, liquidez y comisiones…");
        match client.market_by_slug(&market.slug).await {
            Ok(fresh) => {
                if market.id != fresh.id
                    || market.condition_id != fresh.condition_id
                    || market.question != fresh.question
                    || market.extra.get("description") != fresh.extra.get("description")
                    || market.extra.get("resolutionSource") != fresh.extra.get("resolutionSource")
                    || market.end_date != fresh.end_date
                    || decision::binary_tokens(market)? != decision::binary_tokens(&fresh)?
                {
                    blockers.push("market_changed_during_research".into());
                }
                Some(fresh)
            }
            Err(error) => {
                blockers.push("market_refresh_failed".into());
                quote_errors.insert("market".to_string(), error.to_string());
                None
            }
        }
    } else {
        blockers.push("no_market_selected_use_slug_for_trade_decision".into());
        None
    };
    if let Some(market) = &current_market {
        if market.end_date.0.is_none_or(|end| end <= Utc::now()) {
            blockers.push("missing_or_elapsed_market_deadline".into());
        }
        if !market.active
            || market.closed
            || market.archived
            || market
                .extra
                .get("acceptingOrders")
                .and_then(serde_json::Value::as_bool)
                != Some(true)
        {
            blockers.push("market_not_open_for_orders".into());
        }
        let [yes_token, no_token] = decision::binary_tokens(market)?;
        let (yes, no) = tokio::join!(client.order_book(&yes_token), client.order_book(&no_token));
        for (side, result, token, target) in [
            ("yes", yes, yes_token, &mut yes_quote),
            ("no", no, no_token, &mut no_quote),
        ] {
            match result.and_then(|book| {
                decision::quote(&book, &token, market, &options.policy, Utc::now())
            }) {
                Ok(quote) => *target = Some(quote),
                Err(error) => {
                    quote_errors.insert(side.to_string(), error.to_string());
                }
            }
        }
    }
    let decision = decision::decide(
        &forecast,
        yes_quote.as_ref(),
        no_quote.as_ref(),
        &options.policy,
        blockers,
    )?;
    eprintln!("{}", decision.summary_es);
    let report = json!({
        "rubric_version":"decision_v1", "analysis_version":"directional_v2", "generated_at":Utc::now(), "question":research.question,
        "market_id":current_market.as_ref().map(|m| &m.id), "slug":options.slug,
        "market_snapshot":current_market.as_ref().map(|m| json!({
            "id":m.id,"condition_id":m.condition_id,"question":m.question,
            "outcomes":m.outcomes,"end_date":m.end_date,
            "active":m.active,"closed":m.closed,"archived":m.archived,
            "accepting_orders":m.extra.get("acceptingOrders"),
            "fees_enabled":m.extra.get("feesEnabled"),"fee_schedule":m.extra.get("feeSchedule"),
            "description":m.extra.get("description"),"resolution_source":m.extra.get("resolutionSource")
        })),
        "decision":decision, "forecast":forecast, "policy":options.policy,
        "yes_quote":yes_quote, "no_quote":no_quote, "quote_errors":quote_errors,
        "evidence_selection":selection, "rules_review":rules_review, "research":research,
        "supplemental_searches":supplemental_searches,
        "limitations":["Experimental, uncalibrated forecast bands; not statistical confidence intervals.",
            "Classifier confidence is not the probability of the event.",
            "Google News is a bounded snapshot; inaccessible articles are not read.",
            "Publisher completeness and source independence are not verified.",
            "Quotes are snapshots, not guaranteed fills. No orders are submitted.",
            "No historical calibration or domain-specific sports model has been validated."]
    });
    if let Some(file) = &mut output {
        use std::io::Write;
        file.write_all(polyrover::output::success("ai decide-market", &report)?.as_bytes())
            .map_err(|e| Error::Invalid(format!("cannot save decision report: {e}")))?;
    }
    Ok(report)
}

struct Options {
    slug: Option<String>,
    question: Option<String>,
    news_url: Option<String>,
    output: Option<String>,
    model: String,
    max_age: u32,
    policy: Policy,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = BTreeMap::new();
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            if !matches!(
                flag.as_str(),
                "--slug"
                    | "--question"
                    | "--news-url"
                    | "--output"
                    | "--model"
                    | "--shares"
                    | "--min-edge"
                    | "--max-age-days"
            ) {
                return Err(Error::Invalid(format!("unknown decision option {flag}")));
            }
            let value = args
                .next()
                .filter(|v| !v.trim().is_empty() && !v.starts_with("--"))
                .ok_or_else(|| Error::Invalid(format!("{flag} requires a value")))?;
            if options.insert(flag.as_str(), value.clone()).is_some() {
                return Err(Error::Invalid(format!("duplicate option {flag}")));
            }
        }
        let slug = options.remove("--slug");
        let question = options.remove("--question");
        let news_url = options.remove("--news-url");
        if (slug.is_none() && question.is_none() && news_url.is_none())
            || (question.is_some() && (slug.is_some() || news_url.is_some()))
        {
            return Err(Error::Invalid(
                "provide --slug (optionally --news-url), --question, or --news-url".into(),
            ));
        }
        if let Some(url) = &news_url {
            news::Search::from_url(url)?;
        }
        let mut policy = Policy::default();
        for (flag, target) in [
            ("--shares", &mut policy.shares),
            ("--min-edge", &mut policy.min_edge),
        ] {
            if let Some(value) = options.remove(flag) {
                *target = value
                    .parse()
                    .map_err(|_| Error::Invalid(format!("invalid {flag}")))?;
            }
        }
        policy.validate()?;
        let max_age: u32 = options
            .remove("--max-age-days")
            .unwrap_or("30".into())
            .parse()
            .map_err(|_| Error::Invalid("invalid max age".into()))?;
        if max_age == 0 || max_age > 365 {
            return Err(Error::Invalid("max age must be in 1..=365 days".into()));
        }
        Ok(Self {
            slug,
            question,
            news_url,
            output: options.remove("--output"),
            model: options.remove("--model").unwrap_or("jev-latest".into()),
            max_age,
            policy,
        })
    }
}
