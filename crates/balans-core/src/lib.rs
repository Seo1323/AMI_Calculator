//! Core logic of Balans, independent of the UI and of Tauri.
//!
//! Everything money-related uses [`rust_decimal::Decimal`]; floats are never
//! used for amounts.

pub mod error;
pub mod export;
pub mod fx;
pub mod model;
pub mod money;
pub mod rebalance;

pub use error::{CoreError, Result};
