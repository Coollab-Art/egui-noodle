//! A node-graph canvas for egui where **the application owns the model** and
//! the canvas is a pure view over it. See `README.md` for the philosophy and
//! `docs/architecture.md` for the layering.

#![forbid(unsafe_code)]

mod canvas;
mod model;

pub use canvas::*;
pub use model::*;
