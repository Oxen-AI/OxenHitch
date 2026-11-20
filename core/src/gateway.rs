//! Talks to the Oxen gateway the same way Claude Desktop will: Anthropic-compatible
//! endpoints under `{base_url}/v1`, authenticated with a bearer key.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{Error, Result};

pub const DEFAULT_BASE_URL: &str = "https://hub.oxen.ai/api/ai";

const ANTHROPIC_VERSION: &str = "2023-06-01";
const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    /// A friendlier name, when the gateway provides one.
    pub name: Option<String>,
}

/// The outcome of checking a key against the gateway. Shown to the user, so every
/// failure says what went wrong in plain words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Check {
    Ok { models: Vec<Model> },
    InvalidKey,
    Unreachable { message: String },
    Failed { message: String },
}

/// Where the gateway lives on an Oxen host.
const GATEWAY_PATH: &str = "/api/ai";

/// Turns what a person types into the gateway's base URL.
///
/// A host is enough: `hub.oxen.ai` becomes `https://hub.oxen.ai/api/ai`, and a host on
/// this machine (`localhost:3001`) uses plain http. A full URL is kept as given, minus
/// surrounding space, trailing slashes, and a trailing `/v1` (Claude adds that itself).
pub fn normalize_base_url(input: &str) -> Result<String> {
    let refused = || Error::invalid("Enter a host like hub.oxen.ai or localhost:3001.");
    let text = input.trim();
    if text.is_empty() || text.contains(char::is_whitespace) {
        return Err(refused());
    }

    let (scheme, rest) = match text.split_once("://") {
        Some((scheme @ ("http" | "https"), rest)) => (Some(scheme), rest),
        Some(_) => return Err(refused()),
        None => (None, text),
    };
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix("/v1").unwrap_or(rest).trim_end_matches('/');
    let (host, path) = match rest.find('/') {
        Some(slash) => rest.split_at(slash),
        None => (rest, ""),
    };

    let local = is_local(host);
    // A bare word is far more likely a typo than a machine on the network.
    let host_like = local || host.contains('.') || host.contains(':');
    if host.is_empty() || (scheme.is_none() && !host_like) {
        return Err(refused());
    }

    let scheme = scheme.unwrap_or(if local { "http" } else { "https" });
    let path = if path.is_empty() { GATEWAY_PATH } else { path };
    Ok(format!("{scheme}://{host}{path}"))
}

/// A host on this machine, where there is no TLS to speak of.
fn is_local(host: &str) -> bool {
    if host.starts_with("[::1]") {
        return true;
    }
    let name = host.split(':').next().unwrap_or(host);
    name == "localhost" || name == "0.0.0.0" || name.starts_with("127.")
}

/// Lists the gateway's models, which also proves the key works.
pub async fn check(base_url: &str, api_key: &str) -> Check {
    let response = client()
        .get(format!("{base_url}/v1/models?limit=1000"))
        .bearer_auth(api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .send()
        .await;

    let response = match response {
        Ok(response) => response,
        Err(err) => return unreachable(base_url, &err),
    };
    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Check::InvalidKey;
    }
    if status.as_u16() == 404 {
        // The server is there but has no model list at this path: almost always a wrong address.
        return Check::Failed {
            message: format!(
                "{} is reachable, but there is no Oxen gateway on it. Check the host.",
                host(base_url)
            ),
        };
    }
    if !status.is_success() {
        return Check::Failed {
            message: format!("The gateway answered with an error ({}).", status.as_u16()),
        };
    }
    match response.json::<Value>().await {
        Ok(body) => Check::Ok {
            models: parse_models(&body),
        },
        Err(_) => Check::Failed {
            message: "The gateway's reply was not a model list. Check the gateway address.".into(),
        },
    }
}

/// Sends the smallest possible streaming message, the request Claude depends on.
pub async fn probe_messages(base_url: &str, api_key: &str, model: &str) -> Result<()> {
    let response = client()
        .post(format!("{base_url}/v1/messages"))
        .bearer_auth(api_key)
        .header("anthropic-version", ANTHROPIC_VERSION)
        .json(&json!({
            "model": model,
            "max_tokens": 1,
            "stream": true,
            "messages": [{ "role": "user", "content": "Hi" }]
        }))
        .send()
        .await
        .map_err(|err| Error::invalid(format!("Could not send a test message: {err}")))?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let detail = response
        .json::<Value>()
        .await
        .ok()
        .and_then(|body| {
            body.pointer("/error/message")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| format!("status {}", status.as_u16()));
    Err(Error::invalid(format!(
        "A test message to {model} failed: {detail}"
    )))
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(concat!("oxen-hitch/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("a default HTTP client can always be built")
}

fn unreachable(base_url: &str, err: &reqwest::Error) -> Check {
    let host = host(base_url);
    let message = if err.is_timeout() {
        format!("{host} took too long to answer.")
    } else {
        format!("Could not reach {host}. Check your internet connection.")
    };
    Check::Unreachable { message }
}

fn host(base_url: &str) -> &str {
    base_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(base_url)
}

/// Accepts the Anthropic shape (`data[].id`, `display_name`) and plain OpenAI-style lists.
fn parse_models(body: &Value) -> Vec<Model> {
    let list = body
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| body.as_array());
    list.into_iter()
        .flatten()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_str)?;
            let name = item
                .get("display_name")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty() && *name != id);
            Some(Model {
                id: id.into(),
                name: name.map(String::from),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn base_urls_are_tidied() {
        let ok = |input: &str| normalize_base_url(input).unwrap();
        assert_eq!(ok(" https://hub.oxen.ai/api/ai/ "), "https://hub.oxen.ai/api/ai");
        assert_eq!(ok("https://hub.oxen.ai/api/ai/v1"), "https://hub.oxen.ai/api/ai");
        assert_eq!(ok("https://hub.oxen.ai/api/ai/v1/"), "https://hub.oxen.ai/api/ai");
        assert_eq!(ok("http://localhost:3001/api/ai"), "http://localhost:3001/api/ai");
        assert_eq!(ok("https://proxy.example.com/oxen"), "https://proxy.example.com/oxen");
    }

    #[test]
    fn a_bare_host_becomes_the_gateway_on_that_host() {
        let ok = |input: &str| normalize_base_url(input).unwrap();
        assert_eq!(ok("hub.oxen.ai"), "https://hub.oxen.ai/api/ai");
        assert_eq!(ok(" staging.oxen.ai/ "), "https://staging.oxen.ai/api/ai");
        assert_eq!(ok("https://hub.oxen.ai"), "https://hub.oxen.ai/api/ai");
        assert_eq!(ok("oxen.internal:8443"), "https://oxen.internal:8443/api/ai");
        assert_eq!(ok("hub.oxen.ai/api/ai"), "https://hub.oxen.ai/api/ai");
    }

    #[test]
    fn a_host_on_this_machine_is_reached_without_tls() {
        let ok = |input: &str| normalize_base_url(input).unwrap();
        assert_eq!(ok("localhost:3001"), "http://localhost:3001/api/ai");
        assert_eq!(ok("127.0.0.1:3001"), "http://127.0.0.1:3001/api/ai");
        assert_eq!(ok("localhost"), "http://localhost/api/ai");
        assert_eq!(ok("https://localhost:3001"), "https://localhost:3001/api/ai");
    }

    #[test]
    fn things_that_are_not_hosts_are_refused() {
        for bad in ["", "   ", "oxen", "ftp://x.com", "https://", "hub oxen.ai", "https://a b"] {
            assert!(normalize_base_url(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[tokio::test]
    async fn a_good_key_lists_models() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/ai/v1/models"))
            .and(header("authorization", "Bearer good-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    {"id": "claude-opus-5", "display_name": "Claude Opus 5", "type": "model"},
                    {"id": "llama-4", "type": "model"}
                ],
                "has_more": false
            })))
            .mount(&server)
            .await;

        let result = check(&format!("{}/api/ai", server.uri()), "good-key").await;

        assert_eq!(
            result,
            Check::Ok {
                models: vec![
                    Model { id: "claude-opus-5".into(), name: Some("Claude Opus 5".into()) },
                    Model { id: "llama-4".into(), name: None },
                ]
            }
        );
    }

    #[tokio::test]
    async fn a_rejected_key_is_reported_as_invalid() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        assert_eq!(check(&server.uri(), "bad").await, Check::InvalidKey);
    }

    #[tokio::test]
    async fn a_wrong_address_is_reported_as_a_failure_not_a_bad_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404).set_body_string("\"Not Found\""))
            .mount(&server)
            .await;

        let Check::Failed { message } = check(&server.uri(), "k").await else {
            panic!("a 404 is a failure, not a bad key");
        };
        assert!(message.ends_with(
            "is reachable, but there is no Oxen gateway on it. Check the host."
        ));
    }

    #[tokio::test]
    async fn a_gateway_that_is_down_is_unreachable() {
        // Nothing listens on port 1.
        let result = check("http://127.0.0.1:1", "k").await;
        assert_eq!(
            result,
            Check::Unreachable {
                message: "Could not reach 127.0.0.1:1. Check your internet connection.".into()
            }
        );
    }

    #[tokio::test]
    async fn the_message_probe_streams_one_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("authorization", "Bearer k"))
            .and(body_partial_json(json!({"model": "m", "stream": true, "max_tokens": 1})))
            .respond_with(ResponseTemplate::new(200).set_body_string("event: message_stop\n\n"))
            .mount(&server)
            .await;

        probe_messages(&server.uri(), "k", "m").await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_probe_passes_on_the_gateways_reason() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400).set_body_json(
                json!({"type": "error", "error": {"type": "invalid_request_error", "message": "model not found"}}),
            ))
            .mount(&server)
            .await;

        let err = probe_messages(&server.uri(), "k", "m").await.unwrap_err();
        assert_eq!(err.to_string(), "A test message to m failed: model not found");
    }
}
