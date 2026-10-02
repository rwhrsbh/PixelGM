//! Turns the raw model lists into the ordered chain the game walks through.

use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::BTreeMap;

/// Quality tiers, best first. The chain never mixes them: every flash model is tried before
/// any flash-lite model, and every flash-lite model before any Gemma.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tier {
    Flash,
    FlashLite,
    Gemma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiVersion {
    V1,
    V1Beta,
}

impl ApiVersion {
    pub fn path(self) -> &'static str {
        match self {
            ApiVersion::V1 => "v1",
            ApiVersion::V1Beta => "v1beta",
        }
    }
}

/// One entry of a `models.list` response.
#[derive(Debug, Clone, Deserialize)]
pub struct ListedModel {
    pub name: String,
    #[serde(default, rename = "supportedGenerationMethods")]
    pub methods: Vec<String>,
    #[serde(default, rename = "inputTokenLimit")]
    pub input_token_limit: u32,
    #[serde(default, rename = "outputTokenLimit")]
    pub output_token_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub api: ApiVersion,
    pub tier: Tier,
    pub input_token_limit: u32,
    pub output_token_limit: u32,
}

/// Name tokens of models that answer `generateContent` but are not text chat models, plus
/// `latest` aliases, which point at a model already in the list and share its quota.
const EXCLUDED_TOKENS: &[&str] = &[
    "image", "tts", "live", "audio", "transcribe", "embedding", "robotics", "computer", "omni", "customtools", "latest",
];

struct Rank {
    tier: Tier,
    version: (u32, u32),
    /// Parameter count in billions for Gemma, zero otherwise.
    size: u32,
    preview: bool,
}

fn parse_version(token: &str) -> Option<(u32, u32)> {
    let mut parts = token.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = match parts.next() {
        Some(m) => m.parse().ok()?,
        None => 0,
    };
    Some((major, minor))
}

fn rank(id: &str) -> Option<Rank> {
    let tokens: Vec<&str> = id.split('-').collect();
    if tokens.iter().any(|t| EXCLUDED_TOKENS.contains(t)) {
        return None;
    }
    let version = parse_version(tokens.get(1)?)?;
    let preview = tokens.iter().any(|t| *t == "preview" || *t == "exp");
    match tokens[0] {
        "gemini" if tokens.get(2) == Some(&"flash") => {
            let tier = if tokens.get(3) == Some(&"lite") { Tier::FlashLite } else { Tier::Flash };
            Some(Rank { tier, version, size: 0, preview })
        }
        "gemma" => {
            let size = tokens.iter().find_map(|t| t.strip_suffix('b')?.parse().ok()).unwrap_or(0);
            Some(Rank { tier: Tier::Gemma, version, size, preview })
        }
        _ => None,
    }
}

/// Merges both lists and orders them: tier, then newest version, then largest size, stable
/// before preview. A model present in both lists is used through `v1beta`.
pub fn build_chain(v1: &[ListedModel], v1beta: &[ListedModel]) -> Vec<Model> {
    let mut merged: BTreeMap<String, (ApiVersion, &ListedModel)> = BTreeMap::new();
    for (api, list) in [(ApiVersion::V1, v1), (ApiVersion::V1Beta, v1beta)] {
        for listed in list {
            if !listed.methods.iter().any(|m| m == "generateContent") {
                continue;
            }
            let id = listed.name.strip_prefix("models/").unwrap_or(&listed.name).to_string();
            merged.insert(id, (api, listed));
        }
    }
    let mut ranked: Vec<(Rank, Model)> = merged
        .into_iter()
        .filter_map(|(id, (api, listed))| {
            let rank = rank(&id)?;
            let model = Model {
                id,
                api,
                tier: rank.tier,
                input_token_limit: listed.input_token_limit,
                output_token_limit: listed.output_token_limit,
            };
            Some((rank, model))
        })
        .collect();
    ranked.sort_by(|(a, am), (b, bm)| {
        (a.tier, Reverse(a.version), Reverse(a.size), a.preview, &am.id).cmp(&(b.tier, Reverse(b.version), Reverse(b.size), b.preview, &bm.id))
    });
    ranked.into_iter().map(|(_, m)| m).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(names: &[&str], method: &str) -> Vec<ListedModel> {
        names
            .iter()
            .map(|n| ListedModel { name: format!("models/{n}"), methods: vec![method.to_string()], input_token_limit: 1000, output_token_limit: 100 })
            .collect()
    }

    // The lists a real key returned on 2026-10-02.
    const V1: &[&str] = &[
        "gemini-2.5-flash", "gemini-2.5-pro", "gemma-4-26b-a4b-it", "gemma-4-31b-it", "gemini-2.5-flash-lite", "gemini-2.5-flash-image",
        "gemini-3.1-flash-lite", "gemini-3-pro-image", "gemini-3.1-flash-image", "gemini-3.1-flash-lite-image", "gemini-3.5-flash",
        "gemini-3.5-flash-lite", "gemini-3.6-flash", "gemini-3.7-flash", "gemini-3.8-flash", "antigravity-preview-05-2026",
        "deep-research-preview-04-2026",
    ];
    const V1BETA: &[&str] = &[
        "gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.5-flash-preview-tts", "gemma-4-26b-a4b-it", "gemma-4-31b-it", "gemini-flash-latest",
        "gemini-flash-lite-latest", "gemini-pro-latest", "gemini-2.5-flash-lite", "gemini-2.5-flash-image", "gemini-3-flash-preview",
        "gemini-3.1-pro-preview", "gemini-3.1-pro-preview-customtools", "gemini-3.1-flash-lite-preview", "gemini-3.1-flash-lite",
        "gemini-3.1-flash-image-preview", "gemini-3.1-flash-lite-image", "gemini-3.5-flash", "gemini-3.5-flash-lite",
        "gemini-omni-flash-preview", "gemini-omni-1.1-flash", "gemini-3.5-transcribe", "gemini-3.6-flash", "gemini-3.7-flash",
        "gemini-3.8-flash", "gemini-3.1-flash-tts-preview", "gemini-3.8-flash-tts", "gemini-3.8-flash-lite-tts",
        "gemini-robotics-er-2-preview", "gemini-2.5-computer-use-preview-10-2025", "lyria-3.5",
    ];

    #[test]
    fn real_lists_give_flash_then_lite_then_gemma() {
        let chain = build_chain(&listed(V1, "generateContent"), &listed(V1BETA, "generateContent"));
        let ids: Vec<&str> = chain.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gemini-3.8-flash",
                "gemini-3.7-flash",
                "gemini-3.6-flash",
                "gemini-3.5-flash",
                "gemini-3-flash-preview",
                "gemini-2.5-flash",
                "gemini-3.5-flash-lite",
                "gemini-3.1-flash-lite",
                "gemini-3.1-flash-lite-preview",
                "gemini-2.5-flash-lite",
                "gemma-4-31b-it",
                "gemma-4-26b-a4b-it",
            ]
        );
    }

    #[test]
    fn duplicate_goes_through_v1beta_and_v1_only_stays_v1() {
        let chain = build_chain(&listed(&["gemini-9.1-flash", "gemini-9.0-flash"], "generateContent"), &listed(&["gemini-9.1-flash"], "generateContent"));
        assert_eq!(chain[0].id, "gemini-9.1-flash");
        assert_eq!(chain[0].api, ApiVersion::V1Beta);
        assert_eq!(chain[1].id, "gemini-9.0-flash");
        assert_eq!(chain[1].api, ApiVersion::V1);
    }

    #[test]
    fn future_versions_sort_numerically_not_alphabetically() {
        let chain = build_chain(&[], &listed(&["gemini-3.9-flash", "gemini-3.10-flash", "gemini-10-flash", "gemini-4-flash-lite"], "generateContent"));
        let ids: Vec<&str> = chain.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gemini-10-flash", "gemini-3.10-flash", "gemini-3.9-flash", "gemini-4-flash-lite"]);
    }

    #[test]
    fn stable_goes_before_preview_of_the_same_version() {
        let chain = build_chain(&[], &listed(&["gemini-5.0-flash-preview-01-2027", "gemini-5.0-flash"], "generateContent"));
        assert_eq!(chain[0].id, "gemini-5.0-flash");
    }

    #[test]
    fn models_without_generate_content_are_left_out() {
        let chain = build_chain(&[], &listed(&["gemini-3.8-flash"], "bidiGenerateContent"));
        assert!(chain.is_empty());
    }

    #[test]
    fn larger_gemma_first_and_newer_generation_before_larger_size() {
        let chain = build_chain(&[], &listed(&["gemma-4-31b-it", "gemma-5-12b-it", "gemma-4-26b-a4b-it"], "generateContent"));
        let ids: Vec<&str> = chain.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gemma-5-12b-it", "gemma-4-31b-it", "gemma-4-26b-a4b-it"]);
    }
}
