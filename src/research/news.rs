//! Google News discovery and best-effort reading of every returned publisher page.
//! A feed is a bounded provider snapshot, never an exhaustive search of the web.

use std::{collections::BTreeMap, net::IpAddr, time::Duration};

use chrono::{DateTime, Utc};
use futures_util::{stream, StreamExt};
use reqwest::Url;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{Error, Result};

const MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Search {
    pub query: String,
    pub hl: String,
    pub gl: String,
    pub ceid: String,
}

impl Search {
    pub fn new(query: impl Into<String>) -> Result<Self> {
        let search = Self {
            query: query.into(),
            hl: "en-US".into(),
            gl: "US".into(),
            ceid: "US:en".into(),
        };
        search.validate()?;
        Ok(search)
    }

    pub fn from_url(raw: &str) -> Result<Self> {
        let url =
            Url::parse(raw).map_err(|_| Error::Invalid("invalid Google News search URL".into()))?;
        if url.scheme() != "https"
            || url.host_str() != Some("news.google.com")
            || url.path() != "/search"
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(Error::Invalid(
                "expected https://news.google.com/search?q=...&hl=en-US&gl=US&ceid=US:en".into(),
            ));
        }
        let mut pairs = BTreeMap::new();
        for (key, value) in url.query_pairs() {
            if pairs.insert(key.to_string(), value.to_string()).is_some() {
                return Err(Error::Invalid(
                    "duplicate Google News query parameter".into(),
                ));
            }
        }
        let mut search = Self::new(pairs.remove("q").unwrap_or_default())?;
        for (key, target) in [
            ("hl", &mut search.hl),
            ("gl", &mut search.gl),
            ("ceid", &mut search.ceid),
        ] {
            if let Some(value) = pairs.remove(key) {
                *target = value;
            }
        }
        search.validate()?;
        Ok(search)
    }

    fn validate(&self) -> Result<()> {
        if self.query.trim().is_empty()
            || self.query.len() > 4096
            || [&self.hl, &self.gl, &self.ceid].iter().any(|v| {
                v.is_empty()
                    || v.len() > 32
                    || !v
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ':')
            })
        {
            return Err(Error::Invalid("invalid news query or locale".into()));
        }
        Ok(())
    }

    pub fn url(&self, rss: bool) -> Result<Url> {
        self.validate()?;
        let mut url = Url::parse(if rss {
            "https://news.google.com/rss/search"
        } else {
            "https://news.google.com/search"
        })
        .unwrap();
        url.query_pairs_mut().extend_pairs([
            ("q", &self.query),
            ("hl", &self.hl),
            ("gl", &self.gl),
            ("ceid", &self.ceid),
        ]);
        Ok(url)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewsItem {
    pub id: String,
    pub title: String,
    pub google_url: String,
    pub publisher: String,
    pub publisher_url: String,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReadStatus {
    Extracted,
    Unavailable,
    Duplicate,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Article {
    #[serde(flatten)]
    pub item: NewsItem,
    pub final_url: Option<String>,
    pub retrieved_at: DateTime<Utc>,
    pub status: ReadStatus,
    pub error: Option<String>,
    pub duplicate_of: Option<String>,
    pub extraction_method: Option<String>,
    pub content_sha256: Option<String>,
    pub characters: usize,
    /// Content extraction cannot prove that a publisher served its complete article.
    pub publisher_completeness_verified: bool,
    /// Kept in memory for evaluation, never republished in the JSON report.
    #[serde(skip)]
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    pub search: Search,
    pub search_url: String,
    pub feed_url: String,
    pub retrieved_at: DateTime<Utc>,
    pub provider_snapshot_only: bool,
    pub provider_may_be_capped: bool,
    pub articles: Vec<Article>,
}

pub struct Client;

impl Client {
    /// All feed items are attempted, with at most three articles in flight.
    pub async fn collect(search: Search) -> Result<Collection> {
        let feed_url = search.url(true)?;
        let (_, xml) = fetch(feed_url.clone(), None).await?;
        let items = parse_feed(&xml)?;
        let mut articles = stream::iter(items)
            .map(read_article)
            .buffered(3)
            .collect::<Vec<_>>()
            .await;
        let mut hashes: BTreeMap<String, String> = BTreeMap::new();
        for article in &mut articles {
            if let Some(hash) = &article.content_sha256 {
                if let Some(original) = hashes.get(hash) {
                    article.status = ReadStatus::Duplicate;
                    article.duplicate_of = Some(original.clone());
                    article.text.clear();
                } else {
                    hashes.insert(hash.clone(), article.item.id.clone());
                }
            }
        }
        Ok(Collection {
            search_url: search.url(false)?.to_string(),
            feed_url: feed_url.to_string(),
            search,
            retrieved_at: Utc::now(),
            provider_snapshot_only: true,
            provider_may_be_capped: articles.len() >= 100,
            articles,
        })
    }
}

pub fn parse_feed(xml: &str) -> Result<Vec<NewsItem>> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|_| Error::Invalid("Google News returned invalid RSS".into()))?;
    if !doc.root_element().has_tag_name("rss")
        || !doc.descendants().any(|n| n.has_tag_name("channel"))
    {
        return Err(Error::Invalid(
            "Google News response is not an RSS channel".into(),
        ));
    }
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .enumerate()
        .map(|(i, node)| {
            let field = |name| {
                node.children()
                    .find(|n| n.has_tag_name(name))
                    .and_then(|n| n.text())
                    .unwrap_or("")
                    .to_string()
            };
            NewsItem {
                id: format!("article-{:03}", i + 1),
                title: field("title"),
                google_url: field("link"),
                publisher: field("source"),
                publisher_url: node
                    .children()
                    .find(|n| n.has_tag_name("source"))
                    .and_then(|n| n.attribute("url"))
                    .unwrap_or("")
                    .into(),
                published_at: DateTime::parse_from_rfc2822(&field("pubDate"))
                    .ok()
                    .map(|d| d.with_timezone(&Utc)),
            }
        })
        .collect())
}

/// Read one discovered article, recording access/extraction failures in the result.
pub async fn read_article(item: NewsItem) -> Article {
    let mut article = Article {
        item,
        final_url: None,
        retrieved_at: Utc::now(),
        status: ReadStatus::Unavailable,
        error: None,
        duplicate_of: None,
        extraction_method: None,
        content_sha256: None,
        characters: 0,
        publisher_completeness_verified: false,
        text: String::new(),
    };
    let result = async {
        let url = Url::parse(&article.item.google_url)
            .map_err(|_| Error::Invalid("missing or invalid article URL".into()))?;
        let (url, html) = fetch(url, None).await?;
        let (url, html) = if url.host_str() == Some("news.google.com") {
            let publisher = resolve_google(&url, &html).await?;
            article.final_url = Some(publisher.to_string());
            fetch(publisher, None).await?
        } else {
            (url, html)
        };
        article.final_url = Some(url.to_string());
        if url
            .host_str()
            .is_some_and(|h| h == "news.google.com" || h == "consent.google.com")
        {
            return Err(Error::Invalid(
                "publisher redirect or Google consent could not be resolved".into(),
            ));
        }
        let (text, method) = extract_article(&html)?;
        article.characters = text.chars().count();
        article.content_sha256 = Some(format!("{:x}", Sha256::digest(text.as_bytes())));
        article.text = text;
        article.extraction_method = Some(method);
        article.status = ReadStatus::Extracted;
        Ok(())
    }
    .await;
    if let Err(error) = result {
        article.error = Some(error.to_string());
    }
    article
}

async fn resolve_google(url: &Url, html: &str) -> Result<Url> {
    let id = url
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("");
    let (timestamp, signature) = {
        let document = Html::parse_document(html);
        let selector = Selector::parse("[data-n-a-ts][data-n-a-sg]").unwrap();
        let element = document
            .select(&selector)
            .next()
            .ok_or_else(|| Error::Invalid("Google publisher-link metadata unavailable".into()))?;
        (
            element
                .value()
                .attr("data-n-a-ts")
                .unwrap()
                .parse::<u64>()
                .map_err(|_| Error::Invalid("invalid Google link timestamp".into()))?,
            element.value().attr("data-n-a-sg").unwrap().to_string(),
        )
    };
    // Google's public redirect RPC, not a supported API. Fail visibly if it changes.
    let context = json!([
        [
            "X",
            "X",
            ["X", "X"],
            null,
            null,
            1,
            1,
            "US:en",
            null,
            1,
            null,
            null,
            null,
            null,
            null,
            0,
            1
        ],
        "X",
        "X",
        1,
        [1, 1, 1],
        1,
        1,
        null,
        0,
        0,
        null,
        0
    ]);
    let request = json!(["garturlreq", context, id, timestamp, signature]);
    let payload = json!([[["Fbv4je", request.to_string()]]]).to_string();
    let (_, body) = fetch(
        Url::parse("https://news.google.com/_/DotsSplashUi/data/batchexecute").unwrap(),
        Some(payload),
    )
    .await?;
    parse_google_rpc(&body)
}

fn parse_google_rpc(body: &str) -> Result<Url> {
    for line in body.lines().filter(|l| l.starts_with('[')) {
        if let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(line) {
            for row in rows {
                if row.get(1).and_then(Value::as_str) == Some("Fbv4je") {
                    if let Some(encoded) = row.get(2).and_then(Value::as_str) {
                        if let Ok(answer) = serde_json::from_str::<Value>(encoded) {
                            if answer.get(0).and_then(Value::as_str) == Some("garturlres") {
                                if let Some(raw) = answer.get(1).and_then(Value::as_str) {
                                    let url = Url::parse(raw).map_err(|_| {
                                        Error::Invalid("invalid publisher URL".into())
                                    })?;
                                    validate_url(&url)?;
                                    return Ok(url);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    Err(Error::Invalid(
        "Google publisher redirect unavailable".into(),
    ))
}

/// Fetch public HTTPS only. Resolve and pin public IPs separately for each redirect.
/// No TypeSafe credentials, cookies, or user environment proxy are used here.
async fn fetch(mut url: Url, mut form: Option<String>) -> Result<(Url, String)> {
    for _ in 0..6 {
        validate_url(&url)?;
        let host = url.host_str().unwrap();
        let addresses: Vec<_> = tokio::time::timeout(
            Duration::from_secs(10),
            tokio::net::lookup_host((host, 443)),
        )
        .await
        .map_err(|_| Error::Http("news DNS timeout".into()))?
        .map_err(|_| Error::Http("news DNS lookup failed".into()))?
        .collect();
        if addresses.is_empty() || addresses.iter().any(|a| !public_ip(a.ip())) {
            return Err(Error::Invalid(
                "news URL did not resolve exclusively to public addresses".into(),
            ));
        }
        let http = reqwest::Client::builder().no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20)).resolve_to_addrs(host, &addresses)
            .user_agent("Polyrover/0.2 public-news-research (+https://github.com/TrebuchetDynamics/polyrover)").build()?;
        let request = if let Some(payload) = &form {
            http.post(url.clone()).form(&[("f.req", payload)])
        } else {
            http.get(url.clone())
        };
        let mut response = request
            .send()
            .await
            .map_err(|e| Error::Http(e.without_url().to_string()))?;
        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| Error::Invalid("news redirect has no location".into()))?;
            url = url
                .join(location)
                .map_err(|_| Error::Invalid("invalid news redirect".into()))?;
            form = None;
            continue;
        }
        if !response.status().is_success() {
            return Err(Error::Api {
                status: response.status().as_u16(),
                body: "news page unavailable".into(),
            });
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_BYTES as u64)
        {
            return Err(Error::Invalid(
                "news page exceeds 8 MiB; not truncated or evaluated".into(),
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return Err(Error::Invalid(
                    "news page exceeds 8 MiB; not truncated or evaluated".into(),
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::Invalid("news page encoding is not UTF-8".into()))?;
        return Ok((url, text));
    }
    Err(Error::Invalid("too many news redirects".into()))
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => {
            let [a, b, _, _] = v.octets();
            !(v.is_private()
                || v.is_loopback()
                || v.is_link_local()
                || v.is_broadcast()
                || v.is_documentation()
                || v.is_unspecified()
                || v.is_multicast()
                || a == 0
                || a >= 240
                || a == 100 && (64..=127).contains(&b)
                || a == 198 && (b == 18 || b == 19))
        }
        IpAddr::V6(v) => v
            .to_ipv4_mapped()
            .map(|v| public_ip(IpAddr::V4(v)))
            .unwrap_or_else(|| {
                let s = v.segments();
                (s[0] & 0xe000) == 0x2000 && !(s[0] == 0x2001 && (s[1] == 0xdb8 || s[1] < 0x200))
            }),
    }
}

fn validate_url(url: &Url) -> Result<()> {
    let host = url.host_str().unwrap_or("").trim_end_matches('.');
    if url.scheme() != "https"
        || url.port_or_known_default() != Some(443)
        || !url.username().is_empty()
        || url.password().is_some()
        || host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| !public_ip(ip))
    {
        return Err(Error::Invalid(
            "news retrieval requires a public HTTPS URL without credentials".into(),
        ));
    }
    Ok(())
}

/// Extract the whole available article body, never the RSS description or title.
pub fn extract_article(html: &str) -> Result<(String, String)> {
    let document = Html::parse_document(html);
    for heading in document.select(&Selector::parse("title, h1").unwrap()) {
        let text = heading.text().collect::<Vec<_>>().join(" ").to_lowercase();
        if [
            "access denied",
            "just a moment",
            "verify you are human",
            "robot or human",
            "attention required",
            "captcha",
        ]
        .iter()
        .any(|marker| text.contains(marker))
        {
            return Err(Error::Invalid(
                "publisher access challenge; article not read".into(),
            ));
        }
    }
    let scripts = Selector::parse("script[type='application/ld+json']").unwrap();
    let mut bodies = Vec::new();
    let mut paywalled = false;
    for script in document.select(&scripts) {
        if let Ok(value) = serde_json::from_str::<Value>(&script.inner_html()) {
            article_bodies(&value, &mut bodies, &mut paywalled);
        }
    }
    if paywalled {
        return Err(Error::Invalid(
            "publisher marks article as paywalled; access not bypassed".into(),
        ));
    }
    if let Some(body) = bodies.into_iter().max_by_key(String::len) {
        if body.chars().count() >= 300 {
            return Ok((body, "schema_article_body".into()));
        }
    }
    let mut candidates = Vec::new();
    for scope in ["[itemprop='articleBody']", "article", "main"] {
        for root in document.select(&Selector::parse(scope).unwrap()) {
            let selector = Selector::parse("p, h2, h3, li").unwrap();
            let paragraphs: Vec<_> = root
                .select(&selector)
                .filter(|element| {
                    !element
                        .ancestors()
                        .filter_map(scraper::ElementRef::wrap)
                        .any(|ancestor| {
                            matches!(
                                ancestor.value().name(),
                                "nav" | "footer" | "aside" | "script" | "style"
                            ) || ancestor.value().attr("hidden").is_some()
                                || ancestor.value().attr("aria-hidden") == Some("true")
                        })
                })
                .map(|p| normalize(&p.text().collect::<Vec<_>>().join(" ")))
                .filter(|p| !p.is_empty())
                .collect();
            let text = paragraphs.join("\n\n");
            if text.chars().count() >= 300 {
                candidates.push((text, format!("html_{scope}")));
            }
        }
        if !candidates.is_empty() {
            break;
        }
    }
    candidates
        .into_iter()
        .max_by_key(|(text, _)| text.len())
        .ok_or_else(|| {
            Error::Invalid(
                "article body unavailable (blocked, script-only, paywall, or insufficient text)"
                    .into(),
            )
        })
}

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn article_bodies(value: &Value, bodies: &mut Vec<String>, paywalled: &mut bool) {
    match value {
        Value::Array(values) => {
            for value in values {
                article_bodies(value, bodies, paywalled);
            }
        }
        Value::Object(map) => {
            let article = map.get("@type").is_some_and(|t| {
                t.as_str().is_some_and(|s| s.ends_with("Article"))
                    || t.as_array().is_some_and(|v| {
                        v.iter()
                            .any(|s| s.as_str().is_some_and(|s| s.ends_with("Article")))
                    })
            });
            if article {
                if map
                    .get("isAccessibleForFree")
                    .is_some_and(|v| v == false || v == "false")
                {
                    *paywalled = true;
                }
                if let Some(body) = map.get("articleBody").and_then(Value::as_str) {
                    let fragment = Html::parse_fragment(body);
                    bodies.push(normalize(
                        &fragment.root_element().text().collect::<Vec<_>>().join(" "),
                    ));
                }
            }
            for value in map.values() {
                if value.is_object() || value.is_array() {
                    article_bodies(value, bodies, paywalled);
                }
            }
        }
        _ => (),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_user_query_and_locale_without_query_injection() {
        let raw = "https://news.google.com/search?q=Will+Paris+Saint-Germain+win+the+2026-27+UEFA+Champions+League+Championship?&hl=en-US&gl=US&ceid=US:en";
        let search = Search::from_url(raw).unwrap();
        assert_eq!(
            search.query,
            "Will Paris Saint-Germain win the 2026-27 UEFA Champions League Championship?"
        );
        assert_eq!(search.url(true).unwrap().path(), "/rss/search");
        let custom = Search::new("A&B + C?").unwrap().url(false).unwrap();
        assert_eq!(
            custom.query_pairs().find(|(k, _)| k == "q").unwrap().1,
            "A&B + C?"
        );
        for bad in [
            "https://evil.test/search?q=test",
            "https://news.google.com/search?q=x&q=y",
            "https://news.google.com/search?q=",
        ] {
            assert!(Search::from_url(bad).is_err());
        }
    }

    #[test]
    fn rss_preserves_every_item_including_unreadable_items_and_dates() {
        let xml = r#"<rss><channel><item><title>A &amp; B</title><link>https://example.com/article</link><source url="https://example.com">Example</source><pubDate>Sun, 20 Sep 2026 07:00:00 GMT</pubDate><description>Not article text</description></item><item><title>Missing URL</title></item></channel></rss>"#;
        let items = parse_feed(xml).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "A & B");
        assert!(items[0].published_at.is_some());
        assert_eq!(items[1].google_url, "");
        assert!(parse_feed("<html><body>Blocked</body></html>").is_err());
    }

    #[test]
    fn reads_entire_article_body_and_excludes_navigation() {
        let text = "An attributed fact about this season. ".repeat(400);
        let html = format!("<article><nav><p>Navigation junk</p></nav><p>{text}</p><p>FINAL SENTENCE.</p><footer><p>Footer junk</p></footer></article>");
        let (body, method) = extract_article(&html).unwrap();
        assert!(body.ends_with("FINAL SENTENCE."));
        assert!(body.len() > 12000);
        assert!(!body.contains("junk"));
        assert_eq!(method, "html_article");
    }

    #[test]
    fn jsonld_paywalls_challenges_and_snippets_are_not_full_articles() {
        let body = "Reported match facts. ".repeat(100);
        let value = json!({"@type":"NewsArticle", "articleBody":body});
        let html = format!("<script type='application/ld+json'>{value}</script>");
        assert_eq!(extract_article(&html).unwrap().1, "schema_article_body");
        let value = json!({"@type":"NewsArticle", "articleBody":body, "isAccessibleForFree":false});
        assert!(extract_article(&format!(
            "<script type='application/ld+json'>{value}</script>"
        ))
        .is_err());
        assert!(extract_article(&format!(
            "<title>Access Denied</title><main><p>{body}</p></main>"
        ))
        .is_err());
        assert!(extract_article("<h1>Headline</h1><p>RSS snippet only</p>").is_err());
    }

    #[test]
    fn publisher_rpc_is_parsed_as_json_and_validated() {
        let answer = json!(["garturlres", "https://example.com/story?x=1&y=2", 1]).to_string();
        let body = format!(
            ")]}}'\n\n42\n{}\n",
            json!([["wrb.fr", "Fbv4je", answer, null]])
        );
        assert_eq!(
            parse_google_rpc(&body).unwrap().host_str(),
            Some("example.com")
        );
        assert!(parse_google_rpc("garbage").is_err());
        for url in [
            "http://example.com",
            "https://127.0.0.1",
            "https://[::1]",
            "https://user:password@example.com",
            "https://example.com:444",
            "https://localhost./",
        ] {
            assert!(validate_url(&Url::parse(url).unwrap()).is_err(), "{url}");
        }
        for ip in [
            "10.0.0.1",
            "127.0.0.1",
            "100.64.0.1",
            "169.254.169.254",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
    }
}
