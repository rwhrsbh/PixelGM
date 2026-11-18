//! How much a model is allowed to think. The host picks one setting; how it is expressed
//! depends on the model, and the API only tells by rejecting the request.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ThinkingSetting {
    Off,
    Low,
    #[default]
    Medium,
    High,
    /// Above "high", where a model has it.
    XHigh,
    /// As much as the model can: its highest level where it has one above "high".
    Max,
}

/// The dialects seen in the wild, in the order they are tried. Each model starts at `Level`
/// and moves one step down every time the API rejects its thinking config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ThinkingMode {
    /// `thinkingLevel` with all four levels.
    #[default]
    Level,
    /// `thinkingLevel`, but only `minimal` and `high` are accepted.
    CoarseLevel,
    /// `thinkingBudget` in tokens.
    Budget,
    /// No thinking config at all.
    Unsupported,
}

impl ThinkingMode {
    /// The next dialect to try, or `None` when there is nothing left to drop.
    pub fn downgrade(self) -> Option<ThinkingMode> {
        match self {
            ThinkingMode::Level => Some(ThinkingMode::CoarseLevel),
            ThinkingMode::CoarseLevel => Some(ThinkingMode::Budget),
            ThinkingMode::Budget => Some(ThinkingMode::Unsupported),
            ThinkingMode::Unsupported => None,
        }
    }

    /// The level a setting asks for, as providers name it.
    pub fn level_name(setting: ThinkingSetting) -> &'static str {
        match setting {
            ThinkingSetting::Off => "none",
            ThinkingSetting::Low => "low",
            ThinkingSetting::Medium => "medium",
            ThinkingSetting::High => "high",
            ThinkingSetting::XHigh => "xhigh",
            ThinkingSetting::Max => "max",
        }
    }

    /// The `thinkingConfig` object for a request, with the levels the model takes as its
    /// catalog lists them: the nearest of them to the setting.
    pub fn config_for(self, setting: ThinkingSetting, takes: &[String]) -> Option<Value> {
        match self {
            ThinkingMode::Level | ThinkingMode::CoarseLevel if !takes.is_empty() => Some(json!({ "thinkingLevel": crate::effort::level_among(takes, Self::level_name(setting)) })),
            _ => self.config(setting),
        }
    }

    /// The `thinkingConfig` object for a request, if this dialect sends one.
    pub fn config(self, setting: ThinkingSetting) -> Option<Value> {
        use ThinkingSetting::*;
        match self {
            ThinkingMode::Level => {
                let level = match setting {
                    Off => "minimal",
                    Low => "low",
                    Medium => "medium",
                    High | XHigh | Max => "high",
                };
                Some(json!({ "thinkingLevel": level }))
            }
            ThinkingMode::CoarseLevel => {
                let level = if matches!(setting, Off | Low) { "minimal" } else { "high" };
                Some(json!({ "thinkingLevel": level }))
            }
            ThinkingMode::Budget => {
                // -1 lets the model decide, which is what "high" means on budget models.
                let budget = match setting {
                    Off => 0,
                    Low => 1024,
                    Medium => 4096,
                    High | XHigh | Max => -1,
                };
                Some(json!({ "thinkingBudget": budget }))
            }
            ThinkingMode::Unsupported => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downgrade_walks_every_dialect_once() {
        let mut mode = ThinkingMode::default();
        let mut seen = vec![mode];
        while let Some(next) = mode.downgrade() {
            mode = next;
            seen.push(mode);
        }
        assert_eq!(seen, [ThinkingMode::Level, ThinkingMode::CoarseLevel, ThinkingMode::Budget, ThinkingMode::Unsupported]);
    }

    #[test]
    fn off_is_expressed_in_each_dialect() {
        assert_eq!(ThinkingMode::Level.config(ThinkingSetting::Off), Some(json!({ "thinkingLevel": "minimal" })));
        assert_eq!(ThinkingMode::CoarseLevel.config(ThinkingSetting::Low), Some(json!({ "thinkingLevel": "minimal" })));
        assert_eq!(ThinkingMode::CoarseLevel.config(ThinkingSetting::Medium), Some(json!({ "thinkingLevel": "high" })));
        assert_eq!(ThinkingMode::Budget.config(ThinkingSetting::Off), Some(json!({ "thinkingBudget": 0 })));
        assert_eq!(ThinkingMode::Budget.config(ThinkingSetting::High), Some(json!({ "thinkingBudget": -1 })));
        assert_eq!(ThinkingMode::Unsupported.config(ThinkingSetting::High), None);
    }
}
