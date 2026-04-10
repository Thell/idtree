#![deny(missing_docs)]

//! Implementation of the ID-Tree data structure from:
//! *“Constant-time Connectivity Querying in Dynamic Graphs”* (ACM, 2024).

mod idtree;
pub use crate::idtree::IDTree;

#[cfg(feature = "python")]
/// Python bindings to Rust implementation
pub mod python_idtree;

#[cfg(feature = "cpp")]
/// C++ bindings to reference implementation
pub mod bridge;
