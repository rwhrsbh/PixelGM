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

/// What kind of model a name says it is. The first three are asked without being told to;
/// the other two only when the player puts them in the order by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Class {
    Flash,
    FlashLite,
    Gemma,
    /// The large, paid kind: `pro`, `ultra`, and the code names known to be of it.
    Pro,
    /// A Gemini model whose name says nothing this game knows how to read.
    Unknown,
}

impl Class {
    /// Whether models of this kind are in the chain when the player has not said otherwise.
    pub fn by_default(self) -> bool {
        matches!(self, Class::Flash | Class::FlashLite | Class::Gemma)
    }

    fn tier(self) -> Tier {
        match self {
            Class::FlashLite => Tier::FlashLite,
            Class::Gemma => Tier::Gemma,
            // Put in by hand, a model stands with the best: it is asked where it was put.
            Class::Flash | Class::Pro | Class::Unknown => Tier::Flash,
        }
    }
}

/// One text model of the lists, with what its name was read as.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surveyed {
    pub model: Model,
    pub class: Class,
}

/// The models the player took out of the chain, and the order the rest are asked in: the
/// first is asked first. An empty order leaves the game's own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Choice {
    pub order: Vec<String>,
    pub off: Vec<String>,
}

/// Since the fourth generation a model's kind is a code name, not `flash` or `pro`. These
/// are words of a name, not names: the generation is still read from the lists.
const CODE_NAMES: &[(&str, Class)] = &[("hydrogen", Class::Flash), ("argon", Class::Pro), ("carbon", Class::Pro)];

struct Rank {
    class: Class,
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

/// The kind a Gemini name says, from the words after its version. Words that say how
/// finished a model is, and the numbers of a date, say nothing of its kind.
fn class_of(words: &[&str]) -> Class {
    let words: Vec<&str> = words.iter().copied().filter(|w| !matches!(*w, "preview" | "exp" | "tts") && !w.chars().all(|c| c.is_ascii_digit())).collect();
    if words.iter().any(|w| matches!(*w, "pro" | "ultra")) {
        Class::Pro
    } else if words.contains(&"flash") {
        if words.contains(&"lite") { Class::FlashLite } else { Class::Flash }
    } else {
        words.first().and_then(|word| CODE_NAMES.iter().find(|(name, _)| name == word)).map_or(Class::Unknown, |(_, class)| *class)
    }
}

fn rank_of(tokens: &[&str]) -> Option<Rank> {
    let version = parse_version(tokens.get(1)?)?;
    let preview = tokens.iter().any(|t| *t == "preview" || *t == "exp");
    match tokens[0] {
        "gemini" => Some(Rank { class: class_of(&tokens[2..]), version, size: 0, preview }),
        "gemma" => {
            let size = tokens.iter().find_map(|t| t.strip_suffix('b')?.parse().ok()).unwrap_or(0);
            Some(Rank { class: Class::Gemma, version, size, preview })
        }
        _ => None,
    }
}

fn rank(id: &str) -> Option<Rank> {
    let tokens: Vec<&str> = id.split('-').collect();
    if tokens.iter().any(|t| EXCLUDED_TOKENS.contains(t)) {
        return None;
    }
    rank_of(&tokens)
}

fn merge<'a>(v1: &'a [ListedModel], v1beta: &'a [ListedModel]) -> BTreeMap<String, (ApiVersion, &'a ListedModel)> {
    let mut merged = BTreeMap::new();
    for (api, list) in [(ApiVersion::V1, v1), (ApiVersion::V1Beta, v1beta)] {
        for listed in list {
            if listed.methods.iter().any(|m| m == "generateContent") {
                merged.insert(listed.name.strip_prefix("models/").unwrap_or(&listed.name).to_string(), (api, listed));
            }
        }
    }
    merged
}

/// Every text model of the Gemini and Gemma families in the lists, in the game's own
/// order: flash, flash-lite, Gemma, then the large kind, then what could not be read; in
/// each the newest first, the largest first, stable before preview. A model present in
/// both lists is used through `v1beta`.
pub fn survey(v1: &[ListedModel], v1beta: &[ListedModel]) -> Vec<Surveyed> {
    let mut ranked: Vec<(Rank, Model)> = merge(v1, v1beta)
        .into_iter()
        .filter_map(|(id, (api, listed))| {
            let rank = rank(&id)?;
            let model = Model { id, api, tier: rank.class.tier(), input_token_limit: listed.input_token_limit, output_token_limit: listed.output_token_limit };
            Some((rank, model))
        })
        .collect();
    ranked.sort_by(|(a, am), (b, bm)| {
        (a.class, Reverse(a.version), Reverse(a.size), a.preview, &am.id).cmp(&(b.class, Reverse(b.version), Reverse(b.size), b.preview, &bm.id))
    });
    ranked.into_iter().map(|(rank, model)| Surveyed { model, class: rank.class }).collect()
}

/// The chain as the game makes it when the player has chosen nothing.
pub fn build_chain(v1: &[ListedModel], v1beta: &[ListedModel]) -> Vec<Model> {
    arrange(&survey(v1, v1beta), &Choice::default())
}

/// The chain the game walks: the player's order first of all, without what the player took
/// out. A model the player has not placed — one that appeared since — goes where the
/// game's own order would have it: right before the nearest placed model it outranks. Only the
/// kinds asked by default come in unplaced; the rest wait to be put in by hand.
pub fn arrange(survey: &[Surveyed], choice: &Choice) -> Vec<Model> {
    let place = |id: &str| survey.iter().position(|s| s.model.id == id);
    let off = |id: &str| choice.off.iter().any(|o| o == id);
    let mut chain: Vec<usize> = Vec::new();
    for id in &choice.order {
        if let Some(n) = place(id).filter(|n| !off(id) && !chain.contains(n)) {
            chain.push(n);
        }
    }
    for (n, surveyed) in survey.iter().enumerate() {
        if chain.contains(&n) || off(&surveyed.model.id) || !surveyed.class.by_default() {
            continue;
        }
        // The nearest of the placed models it outranks: it goes right before that one.
        let below = chain.iter().copied().filter(|placed| *placed > n).min();
        let at = below.and_then(|below| chain.iter().position(|placed| *placed == below)).unwrap_or(chain.len());
        chain.insert(at, n);
    }
    chain.into_iter().map(|n| survey[n].model.clone()).collect()
}

/// Name tokens that rule a model out as a voice even when it is marked as one.
const NOT_A_VOICE: &[&str] = &["image", "live", "transcribe", "embedding", "robotics", "computer", "omni", "customtools", "latest"];

/// The models that turn text into speech, in the order they are tried: the same order as
/// for everything else — flash newest to oldest, then flash-lite. No model is named here:
/// a voice is whatever the lists mark with `tts`.
pub fn build_voice_chain(v1: &[ListedModel], v1beta: &[ListedModel]) -> Vec<Model> {
    let mut ranked: Vec<(Rank, Model)> = merge(v1, v1beta)
        .into_iter()
        .filter_map(|(id, (api, listed))| {
            let tokens: Vec<&str> = id.split('-').collect();
            if tokens.first() != Some(&"gemini") || !tokens.contains(&"tts") || tokens.iter().any(|t| NOT_A_VOICE.contains(t)) {
                return None;
            }
            let rank = rank_of(&tokens).filter(|rank| matches!(rank.class, Class::Flash | Class::FlashLite))?;
            let model = Model { id, api, tier: rank.class.tier(), input_token_limit: listed.input_token_limit, output_token_limit: listed.output_token_limit };
            Some((rank, model))
        })
        .collect();
    ranked.sort_by(|(a, am), (b, bm)| (a.class, Reverse(a.version), a.preview, &am.id).cmp(&(b.class, Reverse(b.version), b.preview, &bm.id)));
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

    #[test]
    fn the_voices_are_the_tts_models_flash_before_lite_newest_first() {
        let chain = build_voice_chain(&listed(V1, "generateContent"), &listed(V1BETA, "generateContent"));
        let ids: Vec<&str> = chain.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["gemini-3.8-flash-tts", "gemini-3.1-flash-tts-preview", "gemini-2.5-flash-preview-tts", "gemini-3.8-flash-lite-tts"]);
        assert!(build_chain(&listed(V1, "generateContent"), &listed(V1BETA, "generateContent")).iter().all(|m| !m.id.contains("tts")), "a voice is never asked to run the game");
    }

    // What the fourth generation is called, as far as could be learnt on 2026-11-16.
    const FOURTH: &[&str] = &["gemini-4-hydrogen", "gemini-4-argon", "gemini-4-carbon", "gemini-3.8-flash", "gemini-3.5-flash-lite", "gemini-3.1-pro-preview", "gemma-4-31b-it", "gemini-5-xenon", "gemini-4-hydrogen-preview-11-2026", "gemini-4-hydrogen-tts", "gemini-omni-1.2-flash", "lyria-3.5"];

    fn ids(chain: &[Model]) -> Vec<&str> {
        chain.iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn a_code_name_of_the_flash_kind_is_taken_and_goes_first() {
        let chain = build_chain(&[], &listed(FOURTH, "generateContent"));
        assert_eq!(ids(&chain), ["gemini-4-hydrogen", "gemini-4-hydrogen-preview-11-2026", "gemini-3.8-flash", "gemini-3.5-flash-lite", "gemma-4-31b-it"]);
        assert!(chain.iter().take(3).all(|m| m.tier == Tier::Flash));
    }

    #[test]
    fn the_large_kind_and_names_not_understood_are_listed_but_not_asked_unbidden() {
        let all = survey(&[], &listed(FOURTH, "generateContent"));
        let class = |id: &str| all.iter().find(|s| s.model.id == id).map(|s| s.class);
        assert_eq!(class("gemini-4-argon"), Some(Class::Pro));
        assert_eq!(class("gemini-4-carbon"), Some(Class::Pro));
        assert_eq!(class("gemini-3.1-pro-preview"), Some(Class::Pro));
        assert_eq!(class("gemini-5-xenon"), Some(Class::Unknown));
        // Not a text model of these families at all: not even listed.
        assert_eq!(class("gemini-omni-1.2-flash"), None);
        assert_eq!(class("lyria-3.5"), None);
        assert_eq!(class("gemini-4-hydrogen-tts"), None);
        // What is asked by default comes first in the survey, in the chain's order.
        let unbidden: Vec<&str> = all.iter().filter(|s| s.class.by_default()).map(|s| s.model.id.as_str()).collect();
        assert_eq!(unbidden, ids(&build_chain(&[], &listed(FOURTH, "generateContent"))));
    }

    #[test]
    fn the_players_order_stands_and_what_they_took_out_is_not_asked() {
        let all = survey(&[], &listed(FOURTH, "generateContent"));
        let choice = Choice { order: vec!["gemma-4-31b-it".into(), "gemini-4-argon".into(), "gemini-3.8-flash".into(), "gone-model".into()], off: vec!["gemini-4-hydrogen".into(), "gemini-3.5-flash-lite".into()] };
        // The large model was put in by hand, so it is asked, where it was put. The preview
        // nobody placed goes right before the flash it outranks, not to the top.
        assert_eq!(ids(&arrange(&all, &choice)), ["gemma-4-31b-it", "gemini-4-argon", "gemini-4-hydrogen-preview-11-2026", "gemini-3.8-flash"]);
        // Taken out wins over placed.
        let both = Choice { order: vec!["gemini-4-argon".into()], off: vec!["gemini-4-argon".into()] };
        assert!(!ids(&arrange(&all, &both)).contains(&"gemini-4-argon"));
    }

    #[test]
    fn a_model_that_appears_later_goes_where_the_games_own_order_would_have_it() {
        let before = survey(&[], &listed(&["gemini-3.8-flash", "gemini-3.5-flash-lite", "gemma-4-31b-it"], "generateContent"));
        // The player wants the lite one first.
        let choice = Choice { order: vec!["gemini-3.5-flash-lite".into(), "gemini-3.8-flash".into(), "gemma-4-31b-it".into()], off: Vec::new() };
        assert_eq!(ids(&arrange(&before, &choice)), ["gemini-3.5-flash-lite", "gemini-3.8-flash", "gemma-4-31b-it"]);
        let after = survey(&[], &listed(&["gemini-3.8-flash", "gemini-3.5-flash-lite", "gemma-4-31b-it", "gemini-4-hydrogen", "gemini-4-flash-lite", "gemini-4-argon"], "generateContent"));
        // Each new model goes right before the old one of its kind, wherever the player
        // put that: the new lite before the old lite, the new flash before the old flash.
        // The large one stays out.
        assert_eq!(ids(&arrange(&after, &choice)), ["gemini-4-flash-lite", "gemini-3.5-flash-lite", "gemini-4-hydrogen", "gemini-3.8-flash", "gemma-4-31b-it"]);
    }

    #[test]
    fn a_voice_with_a_code_name_is_a_voice() {
        let voices = build_voice_chain(&[], &listed(&["gemini-4-hydrogen-tts", "gemini-3.8-flash-tts", "gemini-4-argon-tts", "gemini-3.8-flash-lite-tts"], "generateContent"));
        assert_eq!(ids(&voices), ["gemini-4-hydrogen-tts", "gemini-3.8-flash-tts", "gemini-3.8-flash-lite-tts"]);
    }

}
