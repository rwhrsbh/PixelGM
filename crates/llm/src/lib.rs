//! Gemini access for the game master: which models exist, which key and model to try next,
//! and how to talk to them. Nothing here names a concrete model; the chain is derived from
//! the model lists the API returns.

pub mod catalog;
pub mod client;
pub mod dispatch;
pub mod pool;
pub mod thinking;

pub use catalog::{ApiVersion, ListedModel, Model, Tier, build_chain, build_voice_chain};
pub use client::{Failure, GenRequest, HttpResponse, Reply, Speech, Transport, Turn, UreqTransport, Usage};
pub use dispatch::{Dispatcher, GenError, KeyHealth, SavedState};
pub use pool::{Availability, KeyId, Millis, Outcome, PairUsage, Pick, Pool, SavedRest, fingerprint};
pub use thinking::{ThinkingMode, ThinkingSetting};
