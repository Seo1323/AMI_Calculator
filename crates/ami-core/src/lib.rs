//! Core logic of AMI_Calculator, independent of the UI and of Tauri.
//!
//! Everything money-related uses [`rust_decimal::Decimal`]; floats are never
//! used for amounts.

pub mod dashboard;
pub mod error;
pub mod export;
pub mod fx;
pub mod model;
pub mod money;
pub mod portfolio;
pub mod providers;
pub mod rebalance;
pub mod seal;
pub mod store;
pub mod sync;

pub use error::{CoreError, Result};
