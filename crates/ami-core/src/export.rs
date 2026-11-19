//! Portable backup format: a single `.ami` file (pretty-printed JSON)
//! that can be copied to a USB stick and imported on another machine.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::model::*;

pub const FORMAT_ID: &str = "ami-calculator-export";
pub const FORMAT_VERSION: u32 = 1;
pub const FILE_EXTENSION: &str = "ami";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DataSet {
    pub instruments: Vec<Instrument>,
    pub portfolios: Vec<Portfolio>,
    pub targets: Vec<AllocationTarget>,
    pub trades: Vec<Trade>,
    /// Latest known prices (manual ones are user data, so they travel too).
    pub quotes: Vec<Quote>,
    pub fx_rates: Vec<FxRate>,
    /// UI settings, stored as opaque JSON owned by the frontend.
    pub settings: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportBundle {
    pub format: String,
    pub version: u32,
    pub app_version: String,
    pub exported_at: DateTime<Utc>,
    pub data: DataSet,
}

impl ExportBundle {
    pub fn new(app_version: &str, data: DataSet) -> Self {
        ExportBundle {
            format: FORMAT_ID.into(),
            version: FORMAT_VERSION,
            app_version: app_version.into(),
            exported_at: Utc::now(),
            data,
        }
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn from_json(json: &str) -> Result<Self> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|_| CoreError::NotAnExport)?;
        if value.get("format").and_then(|f| f.as_str()) != Some(FORMAT_ID) {
            return Err(CoreError::NotAnExport);
        }
        let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if version > FORMAT_VERSION {
            return Err(CoreError::UnsupportedVersion(version));
        }
        Ok(serde_json::from_value(value)?)
    }

    /// Writes atomically: to a temp file next to the target, then renames.
    pub fn write_file(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("ami.tmp");
        std::fs::write(&tmp, self.to_json()?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn read_file(path: &Path) -> Result<Self> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use rust_decimal_macros::dec;

    #[test]
    fn roundtrip() {
        let mut data = DataSet::default();
        data.portfolios.push(Portfolio {
            meta: Meta::new(),
            name: "IKE".into(),
            base_currency: Currency::new("PLN").unwrap(),
            tolerance_percent: dec!(5.5),
        });
        data.settings = serde_json::json!({ "theme": "dark" });
        let json = ExportBundle::new("0.1.0", data).to_json().unwrap();
        let back = ExportBundle::from_json(&json).unwrap();
        assert_eq!(back.data.portfolios[0].tolerance_percent, dec!(5.5));
        assert_eq!(back.data.settings["theme"], "dark");
    }

    /// Backups made while the app still had a budget module must keep importing.
    #[test]
    fn ignores_unknown_sections_from_older_files() {
        let old = r#"{"format":"ami-calculator-export","version":1,"appVersion":"0.3.0",
            "exportedAt":"2026-11-18T10:00:00Z","data":{"accounts":[{"x":1}],"budgetLimits":[],"portfolios":[]}}"#;
        assert!(
            ExportBundle::from_json(old)
                .unwrap()
                .data
                .portfolios
                .is_empty()
        );
    }

    #[test]
    fn rejects_foreign_and_future_files() {
        assert!(matches!(
            ExportBundle::from_json("{}"),
            Err(CoreError::NotAnExport)
        ));
        assert!(matches!(
            ExportBundle::from_json("nope"),
            Err(CoreError::NotAnExport)
        ));
        let future = r#"{"format":"ami-calculator-export","version":999}"#;
        assert!(matches!(
            ExportBundle::from_json(future),
            Err(CoreError::UnsupportedVersion(999))
        ));
    }
}
