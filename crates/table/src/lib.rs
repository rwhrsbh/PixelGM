//! What a table decides that is neither the world's nor the screen's: who waits for the
//! game master, in what order, and what goes to the master together. Nothing here knows of
//! a window, a wire or a clock: the time is handed in, so every case can be played in a test.

pub mod queue;

pub use queue::{Act, ActId, ActState, Flight, Out, Queues, RequestId, Rules};

/// Milliseconds on whatever clock the caller keeps.
pub type Millis = u64;
