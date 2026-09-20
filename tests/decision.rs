#![cfg(feature = "typesafe")]
use chrono::{Duration, Utc};
use polyrover::{
    decision::*,
    news_research::Report,
    types::{ClobOrderBook, Market},
    typesafe::{Answer, Question, Response, Usage},
};
use serde_json::json;
use std::{collections::BTreeMap, process::Command};

fn market() -> Market {
    serde_json::from_value(
        json!({"id":"market", "conditionId":"condition", "question":"Will X win?",
        "outcomes":["No","Yes"], "clobTokenIds":"[\"no\",\"yes\"]", "feesEnabled":true,
        "feeSchedule":{"rate":0.05,"exponent":1}}),
    )
    .unwrap()
}
fn book() -> ClobOrderBook {
    serde_json::from_value(json!({"asset_id":"yes", "market":"condition", "timestamp":Utc::now().timestamp_millis().to_string(),
        "min_order_size":"5", "asks":[{"price":"0.4","size":"5"},{"price":"0.2","size":"5"}]})).unwrap()
}
fn forecast(low: f64, high: f64) -> Forecast {
    Forecast {
        predicted_outcome: "yes".into(),
        yes_interval: Some([low, high]),
        classifier_confidence: Some(0.9),
        basis: "quantitative_forecasts".into(),
        calibrated: false,
        evaluation: None,
        error: None,
    }
}
fn priced(cost: f64) -> Quote {
    Quote {
        token_id: "yes".into(),
        book_timestamp: Utc::now(),
        shares: 10.0,
        average_ask: cost,
        worst_ask: cost,
        fee_per_share: 0.0,
        total_cost_per_share: cost,
    }
}
fn report() -> Report {
    serde_json::from_value(json!({"question":"Will X win?","search_url":"https://news.google.com/search?q=test",
        "feed_url":"https://news.google.com/rss/search?q=test","retrieved_at":Utc::now(),
        "rubric_version":"news_evidence_v1","provider_snapshot_only":true,"provider_may_be_capped":false,
        "min_confidence":0.8,"max_age_days":30,"collect_only":false,"coverage":{
            "discovered":0,"extracted":0,"unavailable":0,"duplicates":0,"evaluated":0,"evaluation_failures":0,
            "relevant_articles":0,"relevant_publisher_hosts":0,"supports_yes":[],"supports_no":[],"mixed":[],"stale_or_undated":[]},
        "route":"manual_review","reasons":[],"articles":[]})).unwrap()
}

#[test]
fn token_mapping_uses_labels_and_rejects_nonbinary_or_duplicate_tokens() {
    let mut m = market();
    assert_eq!(binary_tokens(&m).unwrap(), ["yes", "no"]);
    m.clob_token_ids = "[\"yes\",\"yes\"]".into();
    assert!(binary_tokens(&m).is_err());
    m = market();
    m.outcomes.0[1] = "Draw".into();
    assert!(binary_tokens(&m).is_err());
}

#[test]
fn depth_quote_sorts_asks_and_includes_market_specific_fees() {
    let q = quote(&book(), "yes", &market(), &Policy::default(), Utc::now()).unwrap();
    assert!((q.average_ask - 0.3).abs() < 1e-10);
    assert!((q.total_cost_per_share - 0.32).abs() < 1e-6);
    assert_eq!(q.worst_ask, 0.4);
}

#[test]
fn quote_rejects_staleness_wrong_tokens_invalid_depth_and_unknown_fees() {
    let mut b = book();
    b.timestamp = (Utc::now() - Duration::minutes(5))
        .timestamp_millis()
        .to_string();
    assert!(quote(&b, "yes", &market(), &Policy::default(), Utc::now()).is_err());
    b = book();
    b.asks[0].size = "NaN".into();
    assert!(quote(&b, "yes", &market(), &Policy::default(), Utc::now()).is_err());
    b = book();
    b.asks.pop();
    assert!(quote(&b, "yes", &market(), &Policy::default(), Utc::now()).is_err());
    assert!(quote(&book(), "no", &market(), &Policy::default(), Utc::now()).is_err());
    let mut m = market();
    m.extra.remove("feeSchedule");
    assert!(quote(&book(), "yes", &m, &Policy::default(), Utc::now()).is_err());
    m.extra.insert("feesEnabled".into(), json!(false));
    assert_eq!(
        quote(&book(), "yes", &m, &Policy::default(), Utc::now())
            .unwrap()
            .fee_per_share,
        0.0
    );
}

#[test]
fn decision_has_all_three_real_branches_and_no_orders() {
    let p = Policy::default();
    let yes = priced(0.3);
    let no = priced(0.7);
    let d = decide(&forecast(0.7, 0.8), Some(&yes), Some(&no), &p, vec![]).unwrap();
    assert_eq!(d.action, Action::BuyYes);
    assert_eq!(d.orders_submitted, 0);
    assert_eq!(
        decide(&forecast(0.0, 0.1), Some(&yes), Some(&no), &p, vec![])
            .unwrap()
            .action,
        Action::BuyNo
    );
    assert_eq!(
        decide(&forecast(0.3, 0.4), Some(&yes), Some(&no), &p, vec![])
            .unwrap()
            .action,
        Action::Wait
    );
}

#[test]
fn likely_no_is_not_automatically_a_good_purchase() {
    let expensive_no = priced(0.95);
    let d = decide(
        &forecast(0.1, 0.2),
        None,
        Some(&expensive_no),
        &Policy::default(),
        vec![],
    )
    .unwrap();
    assert_eq!(d.action, Action::Wait);
    assert!(d.conservative_no_edge.unwrap() < 0.0);
}

#[test]
fn quality_gates_missing_estimates_and_nonfinite_values_fail_closed() {
    let p = Policy::default();
    let yes = priced(0.1);
    assert_eq!(
        decide(
            &forecast(0.8, 0.9),
            Some(&yes),
            None,
            &p,
            vec!["bad_rules".into()]
        )
        .unwrap()
        .action,
        Action::Wait
    );
    assert_eq!(
        decide(
            &Forecast::unavailable("network"),
            Some(&yes),
            None,
            &p,
            vec![]
        )
        .unwrap()
        .action,
        Action::Wait
    );
    assert_eq!(
        decide(&forecast(f64::NAN, 0.9), Some(&yes), None, &p, vec![])
            .unwrap()
            .action,
        Action::Wait
    );
    let mut f = forecast(0.8, 0.9);
    f.classifier_confidence = Some(0.4);
    assert_eq!(
        decide(&f, Some(&yes), None, &p, vec![]).unwrap().action,
        Action::Wait
    );
    let mut bad = p;
    bad.shares = f64::INFINITY;
    assert!(bad.validate().is_err());
}

#[test]
fn forecast_band_is_not_classifier_probability_and_serialization_omits_text() {
    let selection = forecast_request(&report(), None, "test").unwrap();
    let response = Response {
        model: "test".into(),
        usage: Usage {
            input_tokens: 1,
            output_tokens: 1,
        },
        answers: selection
            .request
            .questions
            .iter()
            .map(|(id, q)| {
                let Question::Choice { criteria, .. } = q else {
                    panic!()
                };
                let chosen = if id == "basis" {
                    "quantitative_forecasts"
                } else {
                    "p1"
                };
                (
                    id.clone(),
                    Answer::Choice {
                        choice: chosen.into(),
                        confidence: 0.99,
                        probabilities: criteria
                            .keys()
                            .map(|k| (k.clone(), if k == chosen { 1.0 } else { 0.0 }))
                            .collect(),
                    },
                )
            })
            .collect::<BTreeMap<_, _>>(),
    };
    let f = Forecast::from_response(response, &selection.request).unwrap();
    assert_eq!(f.yes_interval, Some([0.1, 0.2]));
    assert_eq!(f.predicted_outcome, "no");
    assert!(!f.calibrated);
    assert!(serde_json::to_value(selection)
        .unwrap()
        .get("request")
        .is_none());
}

#[test]
fn empty_evidence_does_not_pass_coverage() {
    let r = report();
    let selection = forecast_request(&r, None, "test").unwrap();
    assert_eq!(
        evidence_blockers(&r, &selection, &Policy::default()).len(),
        2
    );
}

#[test]
fn synthesis_preserves_full_text_and_reports_context_exclusions() {
    let mut r = report();
    for (id, text) in [
        ("small", "Complete article".to_owned()),
        ("large", "x".repeat(30_000)),
    ] {
        let mut entry: polyrover::news_research::ArticleReview = serde_json::from_value(json!({
            "article":{"id":id,"title":"Title","google_url":"https://news.google.com/","publisher":"Example",
                "publisher_url":"https://example.com/","published_at":Utc::now()-Duration::hours(1),
                "final_url":"https://example.com/story","retrieved_at":Utc::now(),"status":"extracted",
                "error":null,"duplicate_of":null,"extraction_method":"article","content_sha256":"hash",
                "characters":text.len(),"publisher_completeness_verified":false},
            "chunks_expected":1,"all_extracted_text_evaluated":true,"chunks":[{"index":0,"characters":text.len(),"error":null,
                "evaluation":{"model":"test","usage":{"input_tokens":1,"output_tokens":1},"answers":{"relevance":{"type":"noul","noul":0.99}}}}]})).unwrap();
        entry.article.text = text;
        r.articles.push(entry);
    }
    let selection = forecast_request(&r, None, "test").unwrap();
    assert_eq!(selection.included_ids, vec!["small"]);
    assert_eq!(selection.excluded["large"], "context_budget");
    assert_eq!(
        selection.request.state["articles"][0]["text"],
        "Complete article"
    );
    assert!(selection.state_bytes <= 24_000);
    assert!(evidence_blockers(&r, &selection, &Policy::default())
        .contains(&"relevant_evidence_omitted_from_synthesis".into()));
}

#[test]
fn cli_help_and_bad_arguments_never_require_api_calls() {
    let binary = env!("CARGO_BIN_EXE_polyrover");
    assert!(Command::new(binary)
        .args(["ai", "decide-market", "--help"])
        .output()
        .unwrap()
        .status
        .success());
    for args in [
        vec!["--shares", "NaN", "--question", "test"],
        vec!["--question", "test", "--slug", "test"],
        vec!["--question", "test", "--min-edge", "-1"],
        vec![],
    ] {
        let output = Command::new(binary)
            .args(["ai", "decide-market"])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success());
        let value: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(value["ok"], false);
    }
}

#[test]
fn stale_quotes_cannot_be_reused_for_a_buy_decision() {
    let mut q = priced(0.1);
    q.book_timestamp -= Duration::minutes(3);
    let d = decide(
        &forecast(0.8, 0.9),
        Some(&q),
        None,
        &Policy::default(),
        vec![],
    )
    .unwrap();
    assert_eq!(d.action, Action::Wait);
    assert!(d.reasons.contains(&"no_executable_quote".into()));
}

#[test]
fn dotenv_is_loaded_without_execution_and_environment_takes_precedence() {
    let path = std::env::temp_dir().join(format!(
        "polyrover-env-test-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    std::fs::create_dir(&path).unwrap();
    // A missing market file stops execution before any API call, after loading the key.
    let run = |override_key: Option<&str>| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_polyrover"));
        command
            .current_dir(&path)
            .env_remove("TYPESAFE_API_KEY")
            .args(["ai", "review-market", "--market-file", "absent.json"]);
        if let Some(key) = override_key {
            command.env("TYPESAFE_API_KEY", key);
        }
        String::from_utf8(command.output().unwrap().stderr).unwrap()
    };
    for value in ["test-key", "'test-key'", "\"test-key\""] {
        std::fs::write(
            path.join(".env"),
            format!("export TYPESAFE_API_KEY={value}\n$(touch should-not-exist)\n"),
        )
        .unwrap();
        let error = run(None);
        assert!(error.contains("cannot read market file"), "{error}");
        assert!(!path.join("should-not-exist").exists());
    }
    std::fs::write(
        path.join(".env"),
        "TYPESAFE_API_KEY=one\nTYPESAFE_API_KEY=two\n",
    )
    .unwrap();
    assert!(run(None).contains("duplicate TYPESAFE_API_KEY"));
    assert!(run(Some("override-key")).contains("cannot read market file"));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn decision_output_never_overwrites_an_existing_file() {
    let path = std::env::temp_dir().join(format!(
        "polyrover-output-test-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    std::fs::write(&path, "keep this report").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_polyrover"))
        .env("TYPESAFE_API_KEY", "synthetic-key")
        .args(["ai", "decide-market", "--question", "test", "--output"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot create decision report"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep this report");
    std::fs::remove_file(path).unwrap();
}
