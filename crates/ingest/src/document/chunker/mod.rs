//! Smart semantic chunking engine.
//!
//! Splits extracted documents into overlapping chunks suitable for embedding,
//! dispatching strategy by file type: markdown (heading-aware), PDF (page-aware),
//! and plain text (paragraph/sentence splitting).

mod helpers;
mod strategies;
mod types;

pub use strategies::chunk_document;
pub use types::{Chunk, ChunkConfig};

#[cfg(test)]
mod tests;
