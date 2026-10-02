//! The wire format: building `generateContent` requests and reading what comes back.

use crate::catalog::ListedModel;
use crate::pool::Millis;
use crate::thinking::ThinkingSetting;
use serde_json::{Value, json};
use std::time::Duration;

pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// The network, behind a seam so rotation can be tested without it. `Err` means the server
/// was never reached; any HTTP status, including errors, is an `Ok`.
pub trait Transport: Send + Sync {
    fn get(&self, url: &str, key: &str) -> Result<HttpResponse, String>;
    fn post(&self, url: &str, key: &str, body: &str) -> Result<HttpResponse, String>;
}

pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(timeout: Duration) -> Self {
        Self { agent: ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout(timeout).build() }
    }
}

fn finish(result: Result<ureq::Response, ureq::Error>) -> Result<HttpResponse, String> {
    match result {
        Ok(r) => {
            let status = r.status();
            Ok(HttpResponse { status, body: r.into_string().map_err(|e| e.to_string())? })
        }
        Err(ureq::Error::Status(status, r)) => Ok(HttpResponse { status, body: r.into_string().unwrap_or_default() }),
        Err(e) => Err(e.to_string()),
    }
}

impl Transport for UreqTransport {
    fn get(&self, url: &str, key: &str) -> Result<HttpResponse, String> {
        finish(self.agent.get(url).set("x-goog-api-key", key).call())
    }

    fn post(&self, url: &str, key: &str, body: &str) -> Result<HttpResponse, String> {
        finish(self.agent.post(url).set("x-goog-api-key", key).set("content-type", "application/json").send_string(body))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub from_model: bool,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct GenRequest {
    pub system: Option<String>,
    pub turns: Vec<Turn>,
    /// Ask for JSON output. The answer is still validated by the caller: not every tier obeys.
    pub json: bool,
    pub schema: Option<Value>,
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub thinking: ThinkingSetting,
}

/// The game is for adults and its content toggles live in the game, so the API filters are
/// switched off for every category that can be switched.
const SAFETY_CATEGORIES: &[&str] =
    &["HARM_CATEGORY_HARASSMENT", "HARM_CATEGORY_HATE_SPEECH", "HARM_CATEGORY_SEXUALLY_EXPLICIT", "HARM_CATEGORY_DANGEROUS_CONTENT"];

impl GenRequest {
    pub fn prompt(text: impl Into<String>) -> Self {
        Self { turns: vec![Turn { from_model: false, text: text.into() }], ..Self::default() }
    }

    pub fn body(&self, thinking_config: Option<Value>) -> Value {
        let contents: Vec<Value> =
            self.turns.iter().map(|t| json!({ "role": if t.from_model { "model" } else { "user" }, "parts": [{ "text": t.text }] })).collect();
        let safety: Vec<Value> = SAFETY_CATEGORIES.iter().map(|c| json!({ "category": c, "threshold": "OFF" })).collect();
        let mut config = serde_json::Map::new();
        if self.json {
            config.insert("responseMimeType".into(), json!("application/json"));
        }
        if let Some(schema) = &self.schema {
            config.insert("responseSchema".into(), schema.clone());
        }
        if let Some(t) = self.temperature {
            config.insert("temperature".into(), json!(t));
        }
        if let Some(n) = self.max_output_tokens {
            config.insert("maxOutputTokens".into(), json!(n));
        }
        if let Some(tc) = thinking_config {
            config.insert("thinkingConfig".into(), tc);
        }
        let mut body = json!({ "contents": contents, "safetySettings": safety, "generationConfig": config });
        if let Some(system) = &self.system {
            body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
        }
        body
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub thought_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub text: String,
    pub finish: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    RateLimited { retry_after: Option<Millis>, limit: Option<u32> },
    Overloaded,
    ModelGone,
    KeyRejected,
    /// The model does not take the thinking config it was sent.
    ThinkingRejected,
    /// The model refused the prompt or cut its answer for content reasons.
    Blocked(String),
    BadRequest(String),
}

fn retry_after(error: &Value) -> Option<Millis> {
    let delay = error["details"].as_array()?.iter().find_map(|d| d["retryDelay"].as_str())?;
    let seconds: f64 = delay.strip_suffix('s')?.parse().ok()?;
    Some((seconds * 1000.0).ceil() as Millis)
}

/// The size of the tightest quota the request ran into.
fn quota_limit(error: &Value) -> Option<u32> {
    error["details"]
        .as_array()?
        .iter()
        .filter_map(|d| d["violations"].as_array())
        .flatten()
        .filter_map(|v| v["quotaValue"].as_str()?.parse().ok())
        .min()
}

fn key_is_the_problem(status: u16, error: &Value) -> bool {
    if status == 401 || status == 403 {
        return true;
    }
    let reason_says_so = error["details"].as_array().is_some_and(|ds| ds.iter().any(|d| d["reason"].as_str().is_some_and(|r| r.starts_with("API_KEY"))));
    reason_says_so || error["message"].as_str().is_some_and(|m| m.contains("API key"))
}

fn classify_error(status: u16, body: &str) -> Failure {
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let error = &parsed["error"];
    let message = error["message"].as_str().unwrap_or(body).chars().take(300).collect::<String>();
    match status {
        429 => Failure::RateLimited { retry_after: retry_after(error), limit: quota_limit(error) },
        404 => Failure::ModelGone,
        400..=403 if key_is_the_problem(status, error) => Failure::KeyRejected,
        400 if message.to_lowercase().contains("thinking") => Failure::ThinkingRejected,
        400..=499 => Failure::BadRequest(message),
        _ => Failure::Overloaded,
    }
}

const CONTENT_FINISH_REASONS: &[&str] = &["SAFETY", "PROHIBITED_CONTENT", "BLOCKLIST", "SPII", "RECITATION", "IMAGE_SAFETY"];

pub fn parse_generate(response: &HttpResponse) -> Result<Reply, Failure> {
    if response.status != 200 {
        return Err(classify_error(response.status, &response.body));
    }
    // A 200 that is not JSON is a broken answer from the model's side, same as a server error.
    let parsed: Value = serde_json::from_str(&response.body).map_err(|_| Failure::Overloaded)?;
    if let Some(reason) = parsed["promptFeedback"]["blockReason"].as_str() {
        return Err(Failure::Blocked(reason.to_string()));
    }
    let candidate = &parsed["candidates"][0];
    let finish = candidate["finishReason"].as_str().map(str::to_string);
    let text: String = candidate["content"]["parts"]
        .as_array()
        .map(|parts| parts.iter().filter(|p| p["thought"].as_bool() != Some(true)).filter_map(|p| p["text"].as_str()).collect())
        .unwrap_or_default();
    if text.is_empty() {
        if let Some(reason) = finish.as_deref().filter(|f| CONTENT_FINISH_REASONS.contains(f)) {
            return Err(Failure::Blocked(reason.to_string()));
        }
    }
    let count = |field: &str| parsed["usageMetadata"][field].as_u64().unwrap_or(0) as u32;
    Ok(Reply {
        text,
        finish,
        usage: Usage { prompt_tokens: count("promptTokenCount"), output_tokens: count("candidatesTokenCount"), thought_tokens: count("thoughtsTokenCount") },
    })
}

/// One page of `models.list`: the models and the token of the next page, if any.
pub fn parse_list(response: &HttpResponse) -> Result<(Vec<ListedModel>, Option<String>), Failure> {
    if response.status != 200 {
        return Err(classify_error(response.status, &response.body));
    }
    let parsed: Value = serde_json::from_str(&response.body).map_err(|_| Failure::Overloaded)?;
    let models = serde_json::from_value(parsed["models"].clone()).unwrap_or_default();
    Ok((models, parsed["nextPageToken"].as_str().map(str::to_string)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, body: Value) -> HttpResponse {
        HttpResponse { status, body: body.to_string() }
    }

    #[test]
    fn daily_quota_error_yields_delay_and_limit() {
        // Shape of the answer the free tier gave on 2026-10-02.
        let r = response(
            429,
            json!({ "error": { "code": 429, "message": "You exceeded your current quota", "status": "RESOURCE_EXHAUSTED", "details": [
                { "@type": "type.googleapis.com/google.rpc.QuotaFailure", "violations": [
                    { "quotaId": "GenerateRequestsPerDayPerProjectPerModel-FreeTier", "quotaValue": "20" }
                ] },
                { "@type": "type.googleapis.com/google.rpc.RetryInfo", "retryDelay": "32.4s" }
            ] } }),
        );
        assert_eq!(parse_generate(&r), Err(Failure::RateLimited { retry_after: Some(32_400), limit: Some(20) }));
    }

    #[test]
    fn overload_and_server_errors_are_the_models_problem() {
        let busy = response(503, json!({ "error": { "message": "This model is currently experiencing high demand." } }));
        assert_eq!(parse_generate(&busy), Err(Failure::Overloaded));
        assert_eq!(parse_generate(&response(500, json!({ "error": { "message": "Internal error encountered." } }))), Err(Failure::Overloaded));
        assert_eq!(parse_generate(&HttpResponse { status: 200, body: "<html>".into() }), Err(Failure::Overloaded));
    }

    #[test]
    fn invalid_key_is_told_apart_from_a_bad_request() {
        let bad_key = response(
            400,
            json!({ "error": { "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT",
                "details": [{ "@type": "type.googleapis.com/google.rpc.ErrorInfo", "reason": "API_KEY_INVALID" }] } }),
        );
        assert_eq!(parse_generate(&bad_key), Err(Failure::KeyRejected));
        assert_eq!(parse_generate(&response(403, json!({ "error": { "message": "Permission denied" } }))), Err(Failure::KeyRejected));
        let bad_request = response(400, json!({ "error": { "message": "Request contains an invalid argument." } }));
        assert_eq!(parse_generate(&bad_request), Err(Failure::BadRequest("Request contains an invalid argument.".into())));
    }

    #[test]
    fn thinking_rejections_are_recognised_in_both_wordings() {
        for message in ["Thinking level is not supported for this model.", "Thinking budget is not supported for this model."] {
            assert_eq!(parse_generate(&response(400, json!({ "error": { "message": message } }))), Err(Failure::ThinkingRejected));
        }
    }

    #[test]
    fn missing_model_is_gone() {
        assert_eq!(parse_generate(&response(404, json!({ "error": { "message": "models/x is not found" } }))), Err(Failure::ModelGone));
    }

    #[test]
    fn answer_text_skips_thought_parts_and_carries_usage() {
        let r = response(
            200,
            json!({ "candidates": [{ "finishReason": "STOP", "content": { "parts": [
                { "text": "let me think", "thought": true }, { "text": "{\"a\":" }, { "text": "1}" }
            ] } }], "usageMetadata": { "promptTokenCount": 96, "candidatesTokenCount": 150, "thoughtsTokenCount": 596 } }),
        );
        assert_eq!(
            parse_generate(&r),
            Ok(Reply { text: "{\"a\":1}".into(), finish: Some("STOP".into()), usage: Usage { prompt_tokens: 96, output_tokens: 150, thought_tokens: 596 } })
        );
    }

    #[test]
    fn blocked_prompt_and_blocked_answer_are_reported_as_blocked() {
        let prompt = response(200, json!({ "promptFeedback": { "blockReason": "PROHIBITED_CONTENT" } }));
        assert_eq!(parse_generate(&prompt), Err(Failure::Blocked("PROHIBITED_CONTENT".into())));
        let answer = response(200, json!({ "candidates": [{ "finishReason": "SAFETY" }] }));
        assert_eq!(parse_generate(&answer), Err(Failure::Blocked("SAFETY".into())));
    }

    #[test]
    fn request_body_carries_system_prompt_json_mode_thinking_and_open_filters() {
        let mut req = GenRequest::prompt("go");
        req.system = Some("you are the GM".into());
        req.json = true;
        let body = req.body(Some(json!({ "thinkingLevel": "low" })));
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "you are the GM");
        assert_eq!(body["contents"][0], json!({ "role": "user", "parts": [{ "text": "go" }] }));
        assert_eq!(body["generationConfig"], json!({ "responseMimeType": "application/json", "thinkingConfig": { "thinkingLevel": "low" } }));
        assert_eq!(body["safetySettings"].as_array().unwrap().len(), 4);
        assert!(body["safetySettings"].as_array().unwrap().iter().all(|s| s["threshold"] == "OFF"));
        assert!(GenRequest::prompt("x").body(None).get("systemInstruction").is_none());
    }

    #[test]
    fn list_page_gives_models_and_next_token() {
        let r = response(
            200,
            json!({ "models": [{ "name": "models/gemini-3.8-flash", "supportedGenerationMethods": ["generateContent"], "inputTokenLimit": 1048576, "outputTokenLimit": 65536 }],
                "nextPageToken": "abc" }),
        );
        let (models, next) = parse_list(&r).unwrap();
        assert_eq!(models[0].name, "models/gemini-3.8-flash");
        assert_eq!(models[0].output_token_limit, 65536);
        assert_eq!(next.as_deref(), Some("abc"));
    }
}
