mod fetch;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use ami_core::CoreError;
use ami_core::error::ErrorPayload;
use ami_core::export::ExportBundle;
use ami_core::model::{FxRate, Quote};
use ami_core::money::Currency;
use ami_core::portfolio::{self, PortfolioView};
use ami_core::rebalance::{self, Mode, RebalanceInput, RebalanceResult};
use ami_core::store::{EntityKind, ImportMode, ImportStats, Store, UI_SETTINGS_KEY};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
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

// ---------- portfolios ----------

#[tauri::command]
fn portfolio_view(state: State<'_, AppState>, id: Uuid) -> CmdResult<PortfolioView> {
    Ok(portfolio::view(&state.store(), id)?)
}

#[tauri::command]
fn portfolio_rebalance(
    state: State<'_, AppState>,
    id: Uuid,
    contribution: Decimal,
    mode: Mode,
) -> CmdResult<RebalanceResult> {
    Ok(portfolio::rebalance(
        &state.store(),
        id,
        contribution,
        mode,
    )?)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct TargetInput {
    instrument_id: Uuid,
    target_percent: Decimal,
}

#[tauri::command]
fn portfolio_set_targets(
    state: State<'_, AppState>,
    id: Uuid,
    targets: Vec<TargetInput>,
) -> CmdResult<()> {
    let pairs: Vec<(Uuid, Decimal)> = targets
        .iter()
        .map(|t| (t.instrument_id, t.target_percent))
        .collect();
    Ok(state.store().set_targets(id, &pairs)?)
}

/// A hand-entered price. It keeps the 12-month figures of the previous online quote of the same
/// instrument (they describe the instrument, not the price that was just overwritten).
fn manual_quote(
    previous: Option<&Quote>,
    instrument_id: Uuid,
    price: Decimal,
    currency: Currency,
    now: chrono::DateTime<Utc>,
) -> Quote {
    let previous = previous.filter(|q| q.instrument_id == instrument_id);
    Quote {
        instrument_id,
        price,
        currency,
        source: "manual".into(),
        fetched_at: now,
        return_12m_percent: previous.and_then(|q| q.return_12m_percent),
        dividend_yield_percent: previous.and_then(|q| q.dividend_yield_percent),
    }
}

#[tauri::command]
fn quote_set(
    state: State<'_, AppState>,
    instrument_id: Uuid,
    price: Decimal,
    currency: Currency,
) -> CmdResult<()> {
    let store = state.store();
    let previous = store
        .quotes()?
        .into_iter()
        .find(|q| q.instrument_id == instrument_id);
    let quote = manual_quote(
        previous.as_ref(),
        instrument_id,
        price,
        currency,
        Utc::now(),
    );
    Ok(store.set_quote(&quote)?)
}

/// Current offer of Polish retail treasury bonds: first-period rate, capitalization, price.
/// Series that could not be read are listed in `skipped`.
#[tauri::command]
async fn bond_offers() -> CmdResult<ami_core::providers::BondOffers> {
    fetch::bond_offers()
        .await
        .map_err(|e| CoreError::Provider(e).into())
}

/// Downloads NBP rates and quotes for all instruments with an online source.
#[tauri::command]
async fn quotes_refresh(state: State<'_, AppState>) -> CmdResult<fetch::RefreshReport> {
    let plan = fetch::plan(&state.store())?;
    let data = fetch::download(plan).await;
    Ok(fetch::save(&state.store(), data)?)
}

#[tauri::command]
fn quote_list(state: State<'_, AppState>) -> CmdResult<Vec<Quote>> {
    Ok(state.store().quotes()?)
}

#[tauri::command]
fn fx_list(state: State<'_, AppState>) -> CmdResult<Vec<FxRate>> {
    Ok(state.store().fx_rates()?)
}

#[tauri::command]
fn fx_set(state: State<'_, AppState>, currency: Currency, rate_pln: Decimal) -> CmdResult<()> {
    let rate = FxRate {
        currency,
        rate_pln,
        source: "manual".into(),
        fetched_at: Utc::now(),
    };
    Ok(state.store().set_fx_rate(&rate)?)
}

// ---------- dashboard ----------

#[tauri::command]
fn dashboard(
    state: State<'_, AppState>,
    currency: Currency,
) -> CmdResult<ami_core::dashboard::Dashboard> {
    Ok(ami_core::dashboard::build(&state.store(), &currency)?)
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

/// Contents of a portable `.ami` backup. The UI writes it with the fs plugin, which
/// understands both desktop paths and Android `content://` URIs from the save dialog.
#[tauri::command]
fn export_json(state: State<'_, AppState>) -> CmdResult<String> {
    let data = state.store().export_data()?;
    Ok(ExportBundle::new(env!("CARGO_PKG_VERSION"), data).to_json()?)
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

/// Imports backup contents read by the UI (see `export_json`).
#[tauri::command]
fn import_json(
    state: State<'_, AppState>,
    contents: String,
    mode: ImportMode,
) -> CmdResult<ImportReport> {
    let bundle = ExportBundle::from_json(&contents)?;
    let mut store = state.store();
    let backup = backup_now(&store, &state.data_dir, "before-import")?;
    let stats = store.import_data(&bundle.data, mode)?;
    let settings = (mode == ImportMode::Replace && !bundle.data.settings.is_null())
        .then_some(bundle.data.settings);
    Ok(ImportReport {
        exported_at: bundle.exported_at,
        stats,
        settings,
        backup,
    })
}

fn backup_now(store: &Store, data_dir: &Path, prefix: &str) -> Result<PathBuf, CoreError> {
    let dir = data_dir.join(BACKUPS_DIR);
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!(
        "{prefix}-{}.db",
        Utc::now().format("%Y%m%d-%H%M%S%.3f")
    ));
    store.backup_to(&path)?;
    prune_backups(&dir);
    Ok(path)
}

/// Keeps the newest KEEP_BACKUPS files (names sort by timestamp).
fn prune_backups(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
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
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;
            let store = Store::open(&data_dir.join(DB_FILE))?;
            app.manage(AppState {
                store: Mutex::new(store),
                data_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            rebalance,
            db_list,
            db_save,
            db_delete,
            dashboard,
            portfolio_view,
            portfolio_rebalance,
            portfolio_set_targets,
            quote_set,
            quote_list,
            bond_offers,
            quotes_refresh,
            fx_list,
            fx_set,
            settings_load,
            settings_save,
            export_json,
            import_json,
            data_location,
            app_version
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn online(id: Uuid) -> Quote {
        Quote {
            instrument_id: id,
            price: dec!(100),
            currency: Currency::new("EUR").unwrap(),
            source: "yahoo".into(),
            fetched_at: Utc::now(),
            return_12m_percent: Some(dec!(12.5)),
            dividend_yield_percent: Some(dec!(2.1)),
        }
    }

    #[test]
    fn a_manual_price_keeps_the_twelve_month_figures() {
        let id = Uuid::now_v7();
        let q = manual_quote(
            Some(&online(id)),
            id,
            dec!(90),
            Currency::new("EUR").unwrap(),
            Utc::now(),
        );
        assert_eq!(q.price, dec!(90));
        assert_eq!(q.source, "manual");
        assert_eq!(q.return_12m_percent, Some(dec!(12.5)));
        assert_eq!(q.dividend_yield_percent, Some(dec!(2.1)));
    }

    #[test]
    fn a_manual_price_without_history_has_no_figures() {
        let id = Uuid::now_v7();
        let q = manual_quote(
            None,
            id,
            dec!(90),
            Currency::new("EUR").unwrap(),
            Utc::now(),
        );
        assert_eq!(
            (q.return_12m_percent, q.dividend_yield_percent),
            (None, None)
        );
    }

    #[test]
    fn figures_of_another_instrument_are_not_borrowed() {
        let other = online(Uuid::now_v7());
        let q = manual_quote(
            Some(&other),
            Uuid::now_v7(),
            dec!(90),
            Currency::new("EUR").unwrap(),
            Utc::now(),
        );
        assert_eq!(
            (q.return_12m_percent, q.dividend_yield_percent),
            (None, None)
        );
    }
}
