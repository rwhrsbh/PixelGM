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

    /// The `thinkingConfig` object for a request, if this dialect sends one.
    pub fn config(self, setting: ThinkingSetting) -> Option<Value> {
        use ThinkingSetting::*;
        match self {
            ThinkingMode::Level => {
                let level = match setting {
                    Off => "minimal",
                    Low => "low",
                    Medium => "medium",
                    High => "high",
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
                    High => -1,
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
