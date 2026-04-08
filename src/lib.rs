// #![forbid(unsafe_code)]
#![deny(missing_docs)]

//! Implementation of the ID-Tree data structure from:
//! *“Constant-time Connectivity Querying in Dynamic Graphs”* (ACM, 2024).

mod idtree;

#[cfg(feature = "python")]
mod python_idtree;

pub use crate::idtree::IDTree;

/// Bridge between C++ and Rust
pub mod bridge;
