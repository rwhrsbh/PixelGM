//! Gemini access for the game master: which models exist, which key and model to try next,
//! and how to talk to them. Nothing here names a concrete model; the chain is derived from
//! the model lists the API returns.

pub mod catalog;
pub mod client;
pub mod dispatch;
pub mod pool;
pub mod thinking;

pub use catalog::{ApiVersion, ListedModel, Model, Tier, build_chain};
pub use client::{Failure, GenRequest, HttpResponse, Reply, Transport, Turn, UreqTransport, Usage};
pub use dispatch::{Dispatcher, GenError};
pub use pool::{KeyId, Millis, Outcome, PairUsage, Pick, Pool};
pub use thinking::{ThinkingMode, ThinkingSetting};
