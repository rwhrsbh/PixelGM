//! Any service that speaks the OpenAI chat completions protocol — OpenAI itself, OpenRouter,
//! a local Ollama or LM Studio — as the game master instead of Gemini. One address, one model,
//! one key (none for a local server).

use std::time::Duration;

/// Where to send the requests and with what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Custom {
    /// The address up to and including the version, like `http://localhost:11434/v1`.
    pub url: String,
    pub model: String,
    /// Empty for servers that ask for none.
    pub key: String,
}

impl Custom {
    fn endpoint(&self) -> String {
        let base = self.url.trim().trim_end_matches('/');
        if base.ends_with("/chat/completions") { base.to_string() } else { format!("{base}/chat/completions") }
    }

    /// Asks for a completion of the system words and the prompt; the text of the answer.
    /// A local model may be slow, so it is given minutes.
    pub fn complete(&self, system: &str, prompt: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "model": self.model.trim(),
            "messages": [{ "role": "system", "content": system }, { "role": "user", "content": prompt }],
            "temperature": 0.9,
            "stream": false,
        });
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(15)).timeout(Duration::from_secs(600)).build();
        let mut request = agent.post(&self.endpoint()).set("Content-Type", "application/json");
        if !self.key.trim().is_empty() {
            request = request.set("Authorization", &format!("Bearer {}", self.key.trim()));
        }
        let answer = match request.send_string(&body.to_string()) {
            Ok(answer) => answer,
            Err(ureq::Error::Status(status, answer)) => return Err(format!("{status}: {}", answer.into_string().unwrap_or_default().chars().take(300).collect::<String>())),
            Err(e) => return Err(e.to_string()),
        };
        let text = answer.into_string().map_err(|e| e.to_string())?;
        read_answer(&text)
    }
}

/// The text of a chat completions answer.
pub fn read_answer(body: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("not JSON: {e}"))?;
    if let Some(error) = value.get("error") {
        return Err(error.get("message").and_then(|m| m.as_str()).map_or_else(|| error.to_string(), str::to_string));
    }
    value["choices"][0]["message"]["content"].as_str().map(str::to_string).filter(|t| !t.trim().is_empty()).ok_or_else(|| "the answer had no text".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_takes_the_endpoint_once() {
        let c = |url: &str| Custom { url: url.into(), model: "m".into(), key: String::new() }.endpoint();
        assert_eq!(c("http://localhost:11434/v1"), "http://localhost:11434/v1/chat/completions");
        assert_eq!(c("https://openrouter.ai/api/v1/"), "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(c("http://x/v1/chat/completions"), "http://x/v1/chat/completions");
    }

    #[test]
    fn the_text_and_the_errors_are_read_from_an_answer() {
        assert_eq!(read_answer(r#"{"choices":[{"message":{"role":"assistant","content":"{\"a\":1}"}}]}"#), Ok("{\"a\":1}".to_string()));
        assert_eq!(read_answer(r#"{"error":{"message":"no such model"}}"#), Err("no such model".to_string()));
        assert!(read_answer("<html>").is_err());
    }
}
