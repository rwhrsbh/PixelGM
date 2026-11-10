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

    /// As `post`, for an answer worth reading while it is still being written: the wait for a
    /// first word is the player's setting, and the pieces are put together as they come. A
    /// transport that cannot stream answers whole, which reads the same.
    fn post_stream(&self, url: &str, key: &str, body: &str) -> Result<HttpResponse, String> {
        self.post(url, key, body)
    }
}

pub struct UreqTransport {
    /// How long the connection may be silent, in milliseconds: before the first word of an
    /// answer as well as between the pieces of one. The player sets it, so it can change
    /// while the game runs.
    patience: std::sync::atomic::AtomicU64,
}

impl UreqTransport {
    pub fn new(patience: Duration) -> Self {
        Self { patience: std::sync::atomic::AtomicU64::new(patience.as_millis() as u64) }
    }

    /// Changes how long a silent model is given.
    pub fn set_patience(&self, patience: Duration) {
        self.patience.store(patience.as_millis() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn patience(&self) -> Duration {
        Duration::from_millis(self.patience.load(std::sync::atomic::Ordering::Relaxed).max(1_000))
    }

    /// A connection for one request. No ceiling on the answer itself: a free model may think
    /// for a quarter of an hour, and that is not an error. Only silence is timed.
    fn agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(self.patience()).build()
    }

    /// A connection for the list of models and for checking a key: not a wait for a model's
    /// word, so the player's shortest patience is still given a little room here.
    fn listing_agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout_read(self.patience().max(Duration::from_secs(30))).build()
    }
}

/// Where an answer comes in pieces: the API streams only when asked in the address.
fn stream_url(url: &str) -> String {
    match url.strip_suffix(":generateContent") {
        Some(base) => format!("{base}:streamGenerateContent?alt=sse"),
        None => url.to_string(),
    }
}

/// Puts a streamed answer together into the shape of one that came whole, so that everything
/// above reads it the same way. The thoughts of the model are left out; a stream that broke
/// before its first word is kept as the error, and one that said nothing at all is an overload.
pub fn gather_answer(lines: impl Iterator<Item = std::io::Result<String>>) -> std::io::Result<HttpResponse> {
    let mut text = String::new();
    let mut finish: Option<String> = None;
    let mut usage = Value::Null;
    let mut feedback = Value::Null;
    let mut error: Option<(u16, String)> = None;
    // An answer that came whole although the address asked for pieces.
    let mut whole = String::new();
    for line in lines {
        let line = line?;
        let Some(data) = line.strip_prefix("data:").map(str::trim) else {
            if !line.trim().is_empty() && !line.starts_with(':') && !line.starts_with("event:") {
                whole.push_str(&line);
            }
            continue;
        };
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(piece) = serde_json::from_str::<Value>(data) else { continue };
        if let Some(failed) = piece.get("error") {
            if text.is_empty() {
                let status = failed["code"].as_u64().unwrap_or(500).clamp(400, 599) as u16;
                error = Some((status, piece.to_string()));
            }
            continue;
        }
        if piece.get("promptFeedback").is_some() {
            feedback = piece["promptFeedback"].clone();
        }
        let candidate = &piece["candidates"][0];
        if let Some(reason) = candidate["finishReason"].as_str() {
            finish = Some(reason.to_string());
        }
        if let Some(parts) = candidate["content"]["parts"].as_array() {
            for part in parts.iter().filter(|p| p["thought"].as_bool() != Some(true)) {
                if let Some(word) = part["text"].as_str() {
                    text.push_str(word);
                }
            }
        }
        if piece.get("usageMetadata").is_some() {
            usage = piece["usageMetadata"].clone();
        }
    }
    if let Some((status, body)) = error {
        return Ok(HttpResponse { status, body });
    }
    if !whole.trim().is_empty() {
        return Ok(HttpResponse { status: 200, body: whole });
    }
    if text.is_empty() && finish.is_none() && feedback.is_null() {
        return Ok(HttpResponse { status: 503, body: json!({ "error": { "message": "the stream said nothing" } }).to_string() });
    }
    let mut candidate = json!({ "content": { "parts": [{ "text": text }] } });
    if let Some(reason) = finish {
        candidate["finishReason"] = json!(reason);
    }
    let mut body = json!({ "candidates": [candidate], "usageMetadata": usage });
    if !feedback.is_null() {
        body["promptFeedback"] = feedback;
    }
    Ok(HttpResponse { status: 200, body: body.to_string() })
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
        finish(self.listing_agent().get(url).set("x-goog-api-key", key).call())
    }

    fn post(&self, url: &str, key: &str, body: &str) -> Result<HttpResponse, String> {
        finish(self.agent().post(url).set("x-goog-api-key", key).set("content-type", "application/json").send_string(body))
    }

    fn post_stream(&self, url: &str, key: &str, body: &str) -> Result<HttpResponse, String> {
        // The wait for a first byte is the player's setting, and a socket to an address that
        // never answers can hang far longer than that on its own (Windows gives up after a
        // minute and a half or so). So the request is sent from a thread of its own: if the
        // first byte is overdue, the request is abandoned and the next model is asked, while
        // the thread is left to finish and die on its own.
        let (ready, wait) = std::sync::mpsc::channel::<Result<(), String>>();
        let agent = self.agent();
        let patience = self.patience();
        let (url, key, body) = (stream_url(url), key.to_string(), body.to_string());
        let worker = std::thread::spawn(move || {
            let answer = agent.post(&url).set("x-goog-api-key", &key).set("content-type", "application/json").send_string(&body);
            match answer {
                Ok(answer) => {
                    let _ = ready.send(Ok(()));
                    gather_answer(std::io::BufRead::lines(std::io::BufReader::new(answer.into_reader()))).map_err(|e| e.to_string())
                }
                Err(ureq::Error::Status(status, answer)) => {
                    let _ = ready.send(Ok(()));
                    Ok(HttpResponse { status, body: answer.into_string().unwrap_or_default() })
                }
                Err(error) => {
                    let _ = ready.send(Err(error.to_string()));
                    Err(error.to_string())
                }
            }
        });
        match wait.recv_timeout(patience) {
            Ok(Ok(())) => worker.join().unwrap_or_else(|_| Err("the request was cut short".to_string())),
            Ok(Err(error)) => {
                let _ = worker.join();
                Err(error)
            }
            Err(_) => Err(format!("no first byte of the answer after {} s", patience.as_secs())),
        }
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

/// Speech as it comes back: raw samples, 16 bits, one channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speech {
    pub pcm: Vec<u8>,
    /// Samples per second.
    pub rate: u32,
}

impl Speech {
    /// The samples as a WAV file, which anything can play.
    pub fn wav(&self) -> Vec<u8> {
        let data = self.pcm.len() as u32;
        let mut wav = Vec::with_capacity(44 + self.pcm.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        // PCM, one channel.
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&self.rate.to_le_bytes());
        wav.extend_from_slice(&(self.rate * 2).to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        wav.extend_from_slice(&self.pcm);
        wav
    }

    pub fn seconds(&self) -> f32 {
        self.pcm.len() as f32 / 2.0 / self.rate.max(1) as f32
    }
}

/// The body of a request to have `text` spoken in a named voice.
pub fn speech_body(text: &str, voice: &str) -> Value {
    json!({
        "contents": [{ "parts": [{ "text": text }] }],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": { "voiceConfig": { "prebuiltVoiceConfig": { "voiceName": voice } } }
        }
    })
}

/// Standard base64, padding optional, anything that is not of the alphabet skipped.
pub fn base64_decode(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

/// Reads the answer to a speech request.
pub fn parse_speech(response: &HttpResponse) -> Result<Speech, Failure> {
    if response.status != 200 {
        return Err(classify_error(response.status, &response.body));
    }
    let parsed: Value = serde_json::from_str(&response.body).map_err(|_| Failure::Overloaded)?;
    if let Some(reason) = parsed["promptFeedback"]["blockReason"].as_str() {
        return Err(Failure::Blocked(reason.to_string()));
    }
    let candidate = &parsed["candidates"][0];
    let sound = candidate["content"]["parts"].as_array().and_then(|parts| parts.iter().find_map(|p| p.get("inlineData")));
    let Some(sound) = sound else {
        return Err(match candidate["finishReason"].as_str().filter(|f| CONTENT_FINISH_REASONS.contains(f)) {
            Some(reason) => Failure::Blocked(reason.to_string()),
            // An answer with no sound in it is a broken answer.
            None => Failure::Overloaded,
        });
    };
    let pcm = base64_decode(sound["data"].as_str().unwrap_or_default());
    if pcm.is_empty() {
        return Err(Failure::Overloaded);
    }
    // The type reads like "audio/L16;codec=pcm;rate=24000".
    let rate = sound["mimeType"].as_str().and_then(|mime| mime.split("rate=").nth(1)).and_then(|r| r.split(';').next()).and_then(|r| r.trim().parse().ok()).unwrap_or(24_000);
    Ok(Speech { pcm, rate })
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

    #[test]
    fn base64_reads_with_and_without_padding() {
        assert_eq!(base64_decode("aGVsbG8="), b"hello");
        assert_eq!(base64_decode("aGVsbG8"), b"hello");
        assert_eq!(base64_decode("aGVs\nbG8h"), b"hello!");
        assert_eq!(base64_decode(""), b"");
    }

    #[test]
    fn speech_comes_back_as_samples_with_their_rate_and_makes_a_wav() {
        let body = json!({ "candidates": [{ "content": { "parts": [{ "inlineData": { "mimeType": "audio/L16;codec=pcm;rate=24000", "data": "AAABAAIAAwA=" } }] } }] });
        let speech = parse_speech(&response(200, body)).unwrap();
        assert_eq!((speech.pcm.len(), speech.rate), (8, 24_000));
        let wav = speech.wav();
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 8);
        assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), 24_000);
        assert_eq!(speech.seconds(), 4.0 / 24_000.0);
    }

    #[test]
    fn an_answer_without_sound_is_the_models_fault_and_a_quota_error_is_a_quota_error() {
        let silent = json!({ "candidates": [{ "finishReason": "STOP", "content": { "parts": [{ "text": "no" }] } }] });
        assert_eq!(parse_speech(&response(200, silent)), Err(Failure::Overloaded));
        let quota = json!({ "error": { "code": 429, "message": "quota", "status": "RESOURCE_EXHAUSTED" } });
        assert!(matches!(parse_speech(&response(429, quota)), Err(Failure::RateLimited { .. })));
    }

    #[test]
    fn a_speech_request_names_the_voice_and_asks_for_audio() {
        let body = speech_body("Hello.", "Charon");
        assert_eq!(body["generationConfig"]["responseModalities"][0], "AUDIO");
        assert_eq!(body["generationConfig"]["speechConfig"]["voiceConfig"]["prebuiltVoiceConfig"]["voiceName"], "Charon");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "Hello.");
    }

    fn lines(raw: &[&str]) -> Vec<std::io::Result<String>> {
        raw.iter().map(|line| Ok(line.to_string())).collect()
    }

    #[test]
    fn an_answer_comes_in_pieces_and_its_thoughts_are_left_out() {
        let stream = lines(&[
            r#"data: {"candidates":[{"content":{"parts":[{"text":"The door","thought":true}]},"index":0}]}"#,
            r#"data: {"candidates":[{"content":{"parts":[{"text":"The door"}]},"index":0}]}"#,
            r#"data: {"candidates":[{"content":{"parts":[{"text":" creaks open."}]},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":5,"candidatesTokenCount":9}}"#,
        ]);
        let answer = gather_answer(stream.into_iter()).unwrap();
        assert_eq!(answer.status, 200);
        let reply = parse_generate(&answer).unwrap();
        assert_eq!(reply.text, "The door creaks open.");
        assert_eq!(reply.finish.as_deref(), Some("STOP"));
        assert_eq!(reply.usage.prompt_tokens, 5);
        assert_eq!(reply.usage.output_tokens, 9);
    }

    #[test]
    fn a_stream_that_fails_before_its_first_word_is_read_as_that_error() {
        let stream = lines(&[r#"data: {"error":{"code":429,"message":"quota"}}"#]);
        let answer = gather_answer(stream.into_iter()).unwrap();
        assert_eq!(answer.status, 429);
        assert!(matches!(parse_generate(&answer), Err(Failure::RateLimited { .. })));
    }

    #[test]
    fn a_stream_that_says_nothing_at_all_is_an_overload() {
        let answer = gather_answer(lines(&[": keep-alive"]).into_iter()).unwrap();
        assert_eq!(answer.status, 503);
        assert_eq!(parse_generate(&answer), Err(Failure::Overloaded));
    }

    #[test]
    fn an_answer_that_came_whole_is_read_as_it_is() {
        let body = json!({ "candidates": [{ "finishReason": "STOP", "content": { "parts": [{ "text": "hi" }] } }] }).to_string();
        let answer = gather_answer(lines(&[&body]).into_iter()).unwrap();
        assert_eq!(parse_generate(&answer).unwrap().text, "hi");
    }

    #[test]
    fn an_answer_is_asked_for_in_pieces() {
        assert_eq!(
            stream_url("https://example.test/v1beta/models/gemini-3.0-flash:generateContent"),
            "https://example.test/v1beta/models/gemini-3.0-flash:streamGenerateContent?alt=sse"
        );
    }
}
