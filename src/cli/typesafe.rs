use polyrover::{types::Market, typesafe, Client, Error, Result};

/// Environment takes precedence. Read ONLY this key from local .env; never execute shell code.
pub fn evaluator(config: typesafe::Config) -> Result<typesafe::Client> {
    let provider = std::env::var("POLYROVER_AI_PROVIDER").unwrap_or_else(|_| "typesafe".into());
    if provider.eq_ignore_ascii_case("laya") {
        let mut config = config;
        if config.model == "jev-latest" {
            config.model = std::env::var("LAYA_MODEL").unwrap_or_else(|_| "typed-decisions".into());
        }
        config.base_url =
            std::env::var("LAYA_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".into());
        return typesafe::Client::new_local(config);
    }
    if !provider.eq_ignore_ascii_case("typesafe") {
        return Err(Error::Invalid(
            "POLYROVER_AI_PROVIDER must be typesafe or laya".into(),
        ));
    }
    if std::env::var_os("TYPESAFE_API_KEY").is_some() {
        return typesafe::Client::from_env(config);
    }
    let contents = std::fs::read_to_string(".env")
        .map_err(|_| Error::Invalid("set TYPESAFE_API_KEY or add it to the local .env".into()))?;
    let mut key = None;
    for line in contents.lines() {
        let line = line.trim().strip_prefix("export ").unwrap_or(line.trim());
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != "TYPESAFE_API_KEY" {
            continue;
        }
        if key.is_some() {
            return Err(Error::Invalid("duplicate TYPESAFE_API_KEY in .env".into()));
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        key = Some(value.to_owned());
    }
    typesafe::Client::new(key.as_deref().unwrap_or(""), config)
}

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
        Some(evaluator(config.clone())?)
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
