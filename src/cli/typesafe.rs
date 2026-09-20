use polyrover::{types::Market, typesafe, Client, Error, Result};

pub async fn review(client: &Client, args: &[String]) -> Result<()> {
    let options = Options::parse(args)?;
    let config = typesafe::Config {
        model: options.model,
        ..Default::default()
    };
    // Fail before fetching Gamma data if credentials are missing.
    let evaluator = if options.dry_run {
        None
    } else {
        Some(typesafe::Client::from_env(config.clone())?)
    };
    let market: Market = if let Some(path) = options.market_file {
        let bytes = std::fs::read(path)
            .map_err(|e| Error::Invalid(format!("cannot read market file: {e}")))?;
        serde_json::from_slice(&bytes)?
    } else {
        client
            .market_by_slug(options.slug.as_deref().unwrap())
            .await?
    };
    if let Some(evaluator) = evaluator {
        super::print_success(
            "ai review-market",
            evaluator
                .review_market(&market, options.min_confidence)
                .await?,
        )
    } else {
        super::print_success(
            "ai review-market",
            typesafe::market_review_request(&market, &config.model)?,
        )
    }
}

#[derive(Debug)]
struct Options {
    slug: Option<String>,
    market_file: Option<String>,
    model: String,
    min_confidence: f64,
    dry_run: bool,
}

impl Options {
    fn parse(args: &[String]) -> Result<Self> {
        let mut options = Self {
            slug: None,
            market_file: None,
            model: "jev-latest".into(),
            min_confidence: 0.8,
            dry_run: false,
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut args = args.iter();
        while let Some(flag) = args.next() {
            if !seen.insert(flag) {
                return Err(Error::Invalid(format!("duplicate option {flag}")));
            }
            if flag == "--dry-run" {
                options.dry_run = true;
                continue;
            }
            if !matches!(
                flag.as_str(),
                "--slug" | "--market-file" | "--model" | "--min-confidence"
            ) {
                return Err(Error::Invalid(format!(
                    "unknown ai review-market option {flag}"
                )));
            }
            let value = args
                .next()
                .filter(|v| !v.starts_with("--") && !v.trim().is_empty())
                .ok_or_else(|| Error::Invalid(format!("{flag} requires a value")))?;
            match flag.as_str() {
                "--slug" => options.slug = Some(value.clone()),
                "--market-file" => options.market_file = Some(value.clone()),
                "--model" => options.model = value.clone(),
                "--min-confidence" => {
                    options.min_confidence = value.parse().map_err(|_| {
                        Error::Invalid("--min-confidence requires a number in [0, 1]".into())
                    })?;
                }
                _ => unreachable!(),
            }
        }
        if options.slug.is_some() == options.market_file.is_some() {
            return Err(Error::Invalid(
                "provide exactly one of --slug or --market-file".into(),
            ));
        }
        if !options.min_confidence.is_finite() || !(0.0..=1.0).contains(&options.min_confidence) {
            return Err(Error::Invalid(
                "--min-confidence requires a finite number in [0, 1]".into(),
            ));
        }
        Ok(options)
    }
}
