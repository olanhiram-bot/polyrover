use polyrover::{
    decision_server::{self, DecisionService, Generator},
    Client, Error, Result,
};
use std::{collections::BTreeSet, net::SocketAddr, path::PathBuf, sync::Arc};

pub async fn run(client: &Client, args: &[String]) -> Result<()> {
    let mut bind: SocketAddr = "127.0.0.1:8787".parse().unwrap();
    let mut directory = PathBuf::from("research/decision-api");
    let (mut allowed, mut origins, mut imports) = (BTreeSet::new(), Vec::new(), Vec::new());
    let mut seen = BTreeSet::new();
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if !matches!(
            flag.as_str(),
            "--bind" | "--data-dir" | "--allow-market" | "--allow-origin" | "--import-report"
        ) || (!matches!(
            flag.as_str(),
            "--allow-market" | "--allow-origin" | "--import-report"
        ) && !seen.insert(flag.clone()))
        {
            return Err(Error::Invalid(format!(
                "unknown or duplicate serve option {flag}"
            )));
        }
        let value = args
            .next()
            .filter(|s| !s.is_empty() && !s.starts_with("--"))
            .ok_or_else(|| Error::Invalid(format!("{flag} requires a value")))?;
        match flag.as_str() {
            "--bind" => {
                bind = value
                    .parse()
                    .map_err(|_| Error::Invalid("invalid bind address".into()))?
            }
            "--data-dir" => directory = value.into(),
            "--allow-market" => {
                allowed.insert(value.clone());
            }
            "--import-report" => imports.push(PathBuf::from(value)),
            "--allow-origin" => {
                let url = reqwest::Url::parse(value)
                    .map_err(|_| Error::Invalid("invalid origin".into()))?;
                if !matches!(url.scheme(), "http" | "https")
                    || url.origin().ascii_serialization() != *value
                {
                    return Err(Error::Invalid("origin must be an exact http(s) origin without path, credentials or wildcard".into()));
                }
                origins.push(
                    value
                        .parse()
                        .map_err(|_| Error::Invalid("invalid origin header".into()))?,
                );
            }
            _ => unreachable!(),
        }
    }
    if !allowed.is_empty() {
        super::typesafe_cli::evaluator(Default::default())?;
    }
    let client = client.clone();
    let generator: Generator = Arc::new(move |slug| {
        let client = client.clone();
        Box::pin(
            async move { super::decision_cli::evaluate(&client, &["--slug".into(), slug]).await },
        )
    });
    let service = DecisionService::open(directory, allowed, generator)?;
    for path in imports {
        service.import_report(&path).await?;
    }
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| Error::Invalid(format!("cannot bind decision API: {e}")))?;
    eprintln!(
        "Polyrover decision API: http://{} (no order execution)",
        listener
            .local_addr()
            .map_err(|e| Error::Invalid(e.to_string()))?
    );
    axum::serve(listener, decision_server::router(service, origins))
        .await
        .map_err(|e| Error::Invalid(e.to_string()))
}
