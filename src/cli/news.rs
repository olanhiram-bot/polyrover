use polyrover::{news, news_research, types::Market, typesafe, Client, Error, Result};

pub async fn research(client: &Client, args: &[String]) -> Result<()> {
    let mut options = std::collections::BTreeMap::new();
    let mut collect_only = false;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if flag == "--collect-only" && !collect_only {
            collect_only = true;
            continue;
        }
        if !matches!(
            flag.as_str(),
            "--slug"
                | "--market-file"
                | "--news-url"
                | "--question"
                | "--model"
                | "--min-confidence"
                | "--max-age-days"
                | "--output"
        ) {
            return Err(Error::Invalid(format!(
                "unknown or duplicate research option {flag}"
            )));
        }
        let value = args
            .next()
            .filter(|v| !v.trim().is_empty() && !v.starts_with("--"))
            .ok_or_else(|| Error::Invalid(format!("{flag} requires a value")))?;
        if options.insert(flag.as_str(), value.as_str()).is_some() {
            return Err(Error::Invalid(format!("duplicate option {flag}")));
        }
    }
    if ["--slug", "--market-file", "--news-url", "--question"]
        .iter()
        .filter(|key| options.contains_key(**key))
        .count()
        != 1
    {
        return Err(Error::Invalid(
            "provide exactly one of --slug, --market-file, --news-url, or --question".into(),
        ));
    }
    let confidence: f64 = options
        .get("--min-confidence")
        .unwrap_or(&"0.8")
        .parse()
        .map_err(|_| Error::Invalid("invalid confidence".into()))?;
    let max_age: u32 = options
        .get("--max-age-days")
        .unwrap_or(&"30")
        .parse()
        .map_err(|_| Error::Invalid("invalid max age".into()))?;
    if !confidence.is_finite() || !(0.0..=1.0).contains(&confidence) || max_age == 0 {
        return Err(Error::Invalid(
            "confidence must be in [0,1] and max age must be positive".into(),
        ));
    }
    // Reserve the output before incurring network/API costs; never overwrite a report.
    let mut output = options
        .get("--output")
        .map(|path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
        })
        .transpose()
        .map_err(|e| Error::Invalid(format!("cannot create research report: {e}")))?;
    let model = options.get("--model").copied().unwrap_or("jev-latest");
    let evaluator = if collect_only {
        None
    } else {
        Some(super::typesafe_cli::evaluator(typesafe::Config {
            model: model.into(),
            ..Default::default()
        })?)
    };
    let market: Option<Market> = if let Some(path) = options.get("--market-file") {
        let raw = std::fs::read(path)
            .map_err(|e| Error::Invalid(format!("cannot read market file: {e}")))?;
        Some(serde_json::from_slice(&raw)?)
    } else if let Some(slug) = options.get("--slug") {
        Some(client.market_by_slug(slug).await?)
    } else {
        None
    };
    let search = if let Some(url) = options.get("--news-url") {
        news::Search::from_url(url)?
    } else if let Some(question) = options.get("--question") {
        news::Search::new(*question)?
    } else {
        news::Search::new(
            market
                .as_ref()
                .map(|market| market.question.clone())
                .ok_or_else(|| {
                    Error::Invalid("market is required for this research input".into())
                })?,
        )?
    };
    let collection = news::Client::collect(search).await?;
    let report = news_research::research_for_market(
        collection,
        evaluator.as_ref(),
        model,
        confidence,
        max_age,
        market.as_ref(),
    )
    .await?;
    if let Some(file) = &mut output {
        use std::io::Write;
        file.write_all(polyrover::output::success("ai research-market", &report)?.as_bytes())
            .map_err(|e| Error::Invalid(format!("cannot save report: {e}")))?;
    }
    super::print_success("ai research-market", report)
}
