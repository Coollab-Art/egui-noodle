//! The graph the application owns: nodes with a position and an opaque
//! payload, and the wires between their pins. No egui here - only `emath`
//! types - so it can be built, mutated and tested without a UI.

mod graph;
mod ids;
mod query;

pub use graph::*;
pub use ids::*;
