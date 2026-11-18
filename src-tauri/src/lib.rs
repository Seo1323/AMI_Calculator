use std::path::PathBuf;

use ami_core::error::ErrorPayload;
use ami_core::export::{DataSet, ExportBundle};
use ami_core::rebalance::{self, RebalanceInput, RebalanceResult};

type CmdResult<T> = Result<T, ErrorPayload>;

#[tauri::command]
fn rebalance(input: RebalanceInput) -> CmdResult<RebalanceResult> {
    Ok(rebalance::compute(&input)?)
}

/// Writes a portable `.ami` backup file (e.g. onto a USB stick).
#[tauri::command]
fn export_bundle(path: PathBuf, settings: serde_json::Value) -> CmdResult<()> {
    // TODO(etap 1): fill the data set from the SQLite store.
    let data = DataSet { settings, ..Default::default() };
    ExportBundle::new(env!("CARGO_PKG_VERSION"), data).write_file(&path)?;
    Ok(())
}

#[tauri::command]
fn import_bundle(path: PathBuf) -> CmdResult<ExportBundle> {
    Ok(ExportBundle::read_file(&path)?)
}

#[tauri::command]
fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![rebalance, export_bundle, import_bundle, app_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
