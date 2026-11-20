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

/// Tidies what a person might paste: surrounding space, trailing slashes, and a
/// trailing `/v1` (Claude adds that itself).
pub fn normalize_base_url(input: &str) -> Result<String> {
    let mut url = input.trim().trim_end_matches('/');
    if let Some(stripped) = url.strip_suffix("/v1") {
        url = stripped.trim_end_matches('/');
    }
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    match rest {
        Some(host) if !host.is_empty() && !host.contains(char::is_whitespace) => Ok(url.into()),
        _ => Err(Error::invalid(
            "The gateway address should look like https://hub.oxen.ai/api/ai",
        )),
    }
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
    let host = base_url
        .split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(base_url);
    let message = if err.is_timeout() {
        format!("{host} took too long to answer.")
    } else {
        format!("Could not reach {host}. Check your internet connection.")
    };
    Check::Unreachable { message }
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
    }

    #[test]
    fn things_that_are_not_addresses_are_refused() {
        for bad in ["", "hub.oxen.ai", "ftp://x", "https://", "https://a b"] {
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

        assert!(matches!(check(&server.uri(), "k").await, Check::Failed { .. }));
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
