#[cfg(any(target_arch = "wasm32", feature = "wasm"))]
mod wasm;

#[cfg(any(target_arch = "wasm32", feature = "wasm"))]
pub use wasm::*;
