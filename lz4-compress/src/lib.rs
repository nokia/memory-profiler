//! Pure Rust implementation of LZ4 compression.
//!
//! A detailed explanation of the algorithm can be found [here](http://ticki.github.io/blog/how-lz4-works/).

#![warn(missing_docs)]

extern crate byteorder;
#[macro_use]
extern crate quick_error;

mod compress;
mod decompress;
#[cfg(test)]
mod tests;

pub use compress::{compress, compress_into};
pub use decompress::{decompress, decompress_into};
