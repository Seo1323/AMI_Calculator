//! Core logic of AMI_Calculator, independent of the UI and of Tauri.
//!
//! Everything money-related uses [`rust_decimal::Decimal`]; floats are never
//! used for amounts.

pub mod budget;
pub mod error;
pub mod export;
pub mod fx;
pub mod model;
pub mod money;
pub mod portfolio;
pub mod rebalance;
pub mod store;

pub use error::{CoreError, Result};
