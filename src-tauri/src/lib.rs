use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use ami_core::CoreError;
use ami_core::error::ErrorPayload;
use ami_core::export::ExportBundle;
use ami_core::rebalance::{self, RebalanceInput, RebalanceResult};
use ami_core::store::{EntityKind, ImportMode, ImportStats, Store, UI_SETTINGS_KEY};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tauri::{Manager, State};
use uuid::Uuid;

type CmdResult<T> = Result<T, ErrorPayload>;

const DB_FILE: &str = "ami.db";
const BACKUPS_DIR: &str = "backups";
const KEEP_BACKUPS: usize = 10;

struct AppState {
    store: Mutex<Store>,
    data_dir: PathBuf,
}

impl AppState {
    fn store(&self) -> MutexGuard<'_, Store> {
        // A panic while holding the lock cannot leave SQLite inconsistent, so recover.
        self.store.lock().unwrap_or_else(|e| e.into_inner())
    }
}

// ---------- calculator ----------

#[tauri::command]
fn rebalance(input: RebalanceInput) -> CmdResult<RebalanceResult> {
    Ok(rebalance::compute(&input)?)
}

// ---------- records ----------

#[tauri::command]
fn db_list(state: State<'_, AppState>, kind: EntityKind) -> CmdResult<Vec<Value>> {
    Ok(state.store().list_json(kind)?)
}

#[tauri::command]
fn db_save(state: State<'_, AppState>, kind: EntityKind, value: Value) -> CmdResult<Value> {
    Ok(state.store().save_json(kind, value)?)
}

#[tauri::command]
fn db_delete(state: State<'_, AppState>, kind: EntityKind, id: Uuid) -> CmdResult<bool> {
    Ok(state.store().delete_json(kind, id)?)
}

// ---------- settings ----------

#[tauri::command]
fn settings_load(state: State<'_, AppState>) -> CmdResult<Option<Value>> {
    Ok(state.store().get_setting(UI_SETTINGS_KEY)?)
}

#[tauri::command]
fn settings_save(state: State<'_, AppState>, value: Value) -> CmdResult<()> {
    Ok(state.store().set_setting(UI_SETTINGS_KEY, &value)?)
}

// ---------- backup file (.ami) ----------

/// Writes everything to a portable `.ami` file (e.g. onto a USB stick).
#[tauri::command]
fn export_bundle(state: State<'_, AppState>, path: PathBuf) -> CmdResult<()> {
    let data = state.store().export_data()?;
    ExportBundle::new(env!("CARGO_PKG_VERSION"), data).write_file(&path)?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReport {
    exported_at: DateTime<Utc>,
    stats: ImportStats,
    /// UI settings from the file, to apply right away (replace mode only).
    settings: Option<Value>,
    /// Automatic copy of the database taken before the import.
    backup: PathBuf,
}

#[tauri::command]
fn import_bundle(state: State<'_, AppState>, path: PathBuf, mode: ImportMode) -> CmdResult<ImportReport> {
    let bundle = ExportBundle::read_file(&path)?;
    let mut store = state.store();
    let backup = backup_now(&store, &state.data_dir, "before-import")?;
    let stats = store.import_data(&bundle.data, mode)?;
    let settings = (mode == ImportMode::Replace && !bundle.data.settings.is_null()).then_some(bundle.data.settings);
    Ok(ImportReport { exported_at: bundle.exported_at, stats, settings, backup })
}

fn backup_now(store: &Store, data_dir: &Path, prefix: &str) -> Result<PathBuf, CoreError> {
    let dir = data_dir.join(BACKUPS_DIR);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{prefix}-{}.db", Utc::now().format("%Y%m%d-%H%M%S%.3f")));
    store.backup_to(&path)?;
    prune_backups(&dir);
    Ok(path)
}

/// Keeps the newest KEEP_BACKUPS files (names sort by timestamp).
fn prune_backups(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "db"))
        .collect();
    files.sort();
    let excess = files.len().saturating_sub(KEEP_BACKUPS);
    for old in &files[..excess] {
        let _ = fs::remove_file(old);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DataLocation {
    database: PathBuf,
    backups: PathBuf,
    schema_version: i64,
}

#[tauri::command]
fn data_location(state: State<'_, AppState>) -> CmdResult<DataLocation> {
    Ok(DataLocation {
        database: state.data_dir.join(DB_FILE),
        backups: state.data_dir.join(BACKUPS_DIR),
        schema_version: state.store().schema_version()?,
    })
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
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let store = Store::open(&data_dir.join(DB_FILE))?;
            app.manage(AppState { store: Mutex::new(store), data_dir });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            rebalance,
            db_list,
            db_save,
            db_delete,
            settings_load,
            settings_save,
            export_bundle,
            import_bundle,
            data_location,
            app_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
