pub mod display;
mod fetch;
mod update;

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
use ami_core::sync::SyncOutcome;
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
    let store = state.store();
    let v = portfolio::view(&store, id)?;
    portfolio::record_today(&store, &v)?;
    Ok(v)
}

/// One portfolio's trades (the detail page needs no others).
#[tauri::command]
fn portfolio_trades(
    state: State<'_, AppState>,
    id: Uuid,
) -> CmdResult<Vec<ami_core::model::Trade>> {
    Ok(state.store().trades_of(id)?)
}

#[tauri::command]
fn portfolio_history(state: State<'_, AppState>, id: Uuid) -> CmdResult<portfolio::History> {
    Ok(portfolio::history(&state.store(), id)?)
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

/// Current price of one instrument from its online source, to prefill a holding.
#[tauri::command]
async fn quote_preview(
    symbol: String,
    source: ami_core::model::QuoteSource,
    currency: Currency,
) -> CmdResult<fetch::QuotePreview> {
    fetch::quote_preview(source, symbol.trim(), &currency)
        .await
        .map_err(|e| CoreError::Provider(e).into())
}

/// Current offer of Polish retail treasury bonds: first-period rate, capitalization, price.
/// Series that could not be read are listed in `skipped`.
#[tauri::command]
async fn bond_offers() -> CmdResult<ami_core::providers::BondOffers> {
    fetch::bond_offers()
        .await
        .map_err(|e| CoreError::Provider(e).into())
}

/// Looks for a newer release in the public releases repository.
#[tauri::command]
async fn update_check() -> CmdResult<Option<update::UpdateInfo>> {
    Ok(update::check().await.map_err(CoreError::Provider)?)
}

/// Opens a release page in the browser (desktop; Android does it through its own bridge).
#[tauri::command]
fn update_open(url: String) -> CmdResult<()> {
    if !update::page_allowed(&url) {
        return Err(CoreError::Validation("this page may not be opened".into()).into());
    }
    #[cfg(desktop)]
    open::that(&url).map_err(CoreError::from)?;
    Ok(())
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
    let backup = backup_now(&store, &state.data_dir.join(BACKUPS_DIR), "before-import")?;
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

fn backup_now(store: &Store, dir: &Path, prefix: &str) -> Result<PathBuf, CoreError> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!(
        "{prefix}-{}.db",
        Utc::now().format("%Y%m%d-%H%M%S%.3f")
    ));
    store.backup_to(&path)?;
    prune_backups(dir);
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

// ---------- sync through a shared file ----------

/// One sync round. Safety copies go to their own folder so frequent syncs cannot push the
/// user's own backups out of the newest-10 limit.
fn run_sync(
    state: &AppState,
    incoming: Option<&str>,
    passphrase: Option<&str>,
    allow_plain: bool,
    rekey: Option<&str>,
) -> Result<SyncOutcome, CoreError> {
    let dir = state.data_dir.join(BACKUPS_DIR).join("sync");
    let mut store = state.store();
    ami_core::sync::sync(
        &mut store,
        env!("CARGO_PKG_VERSION"),
        incoming,
        passphrase,
        allow_plain,
        rekey,
        |s| backup_now(s, &dir, "before-sync").map(drop),
    )
}

/// For callers that do the file access themselves (Android content:// URIs): merges the text
/// read from the sync file and returns the text to write back, if any.
#[tauri::command]
fn sync_text(
    state: State<'_, AppState>,
    contents: Option<String>,
    passphrase: Option<String>,
    allow_plain: bool,
    rekey: Option<String>,
) -> CmdResult<SyncOutcome> {
    Ok(run_sync(
        &state,
        contents.as_deref(),
        passphrase.as_deref(),
        allow_plain,
        rekey.as_deref(),
    )?)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncReport {
    stats: ImportStats,
    wrote: bool,
}

/// Desktop: reads and writes the sync file here (not through the fs plugin, whose access to a
/// picked path ends with the session), so the folder keeps working after a restart.
#[tauri::command]
fn sync_file(
    state: State<'_, AppState>,
    path: PathBuf,
    passphrase: Option<String>,
    allow_plain: bool,
    rekey: Option<String>,
) -> CmdResult<SyncReport> {
    Ok(sync_at(
        &state,
        &path,
        passphrase.as_deref(),
        allow_plain,
        rekey.as_deref(),
    )?)
}

fn sync_at(
    state: &AppState,
    path: &Path,
    passphrase: Option<&str>,
    allow_plain: bool,
    rekey: Option<&str>,
) -> Result<SyncReport, CoreError> {
    // The path comes from the webview: it may only name a .ami file, so a script that got into
    // the page cannot make the app write the portfolio export wherever it likes.
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("ami"))
    {
        return Err(CoreError::Validation(
            "the sync file must have the .ami extension".into(),
        ));
    }
    // Read through the open handle with a cap: a symlink to /dev/zero, a FIFO or a file that grows
    // meanwhile cannot make the app read without limit.
    let text = match fs::File::open(path) {
        Ok(f) => {
            let mut text = String::new();
            let cap = ami_core::sync::MAX_FILE_BYTES as u64 + 1;
            std::io::Read::read_to_string(&mut std::io::Read::take(f, cap), &mut text)?;
            Some(text)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let out = run_sync(state, text.as_deref(), passphrase, allow_plain, rekey)?;
    if let Some(json) = &out.outgoing {
        // Temp file + rename: a sync tool never uploads a half-written file.
        // The name is guessable: remove what is there and create it exclusively, so a planted
        // symlink at that name is never written through.
        let tmp = path.with_extension("ami.tmp");
        let _ = fs::remove_file(&tmp);
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        std::io::Write::write_all(&mut f, json.as_bytes())?;
        drop(f);
        fs::rename(&tmp, path)?;
    }
    Ok(SyncReport {
        stats: out.stats,
        wrote: out.outgoing.is_some(),
    })
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
            portfolio_history,
            portfolio_trades,
            portfolio_rebalance,
            portfolio_set_targets,
            quote_set,
            quote_list,
            bond_offers,
            quote_preview,
            quotes_refresh,
            update_check,
            update_open,
            fx_list,
            fx_set,
            settings_load,
            settings_save,
            export_json,
            import_json,
            sync_text,
            sync_file,
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

    /// A device with its own folder; the folder is removed when the `TempDir` is dropped.
    fn device() -> (AppState, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let state = AppState {
            store: Mutex::new(store),
            data_dir: dir.path().to_path_buf(),
        };
        (state, dir)
    }

    fn add_portfolio(state: &AppState, name: &str) {
        use ami_core::model::{AccountType, Meta, Portfolio};
        state
            .store()
            .save(Portfolio {
                meta: Meta::new(),
                name: name.into(),
                base_currency: Currency::new("PLN").unwrap(),
                tolerance_percent: dec!(5),
                account_type: AccountType::default(),
            })
            .unwrap();
    }

    fn names(state: &AppState) -> Vec<String> {
        let mut v: Vec<_> = state
            .store()
            .list::<ami_core::model::Portfolio>()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        v.sort();
        v
    }

    #[test]
    fn two_devices_share_one_file_in_a_folder() {
        let (a, a_dir) = device();
        let (b, _b_dir) = device();
        let shared = a_dir.path().join("ami-sync.ami"); // a folder both devices can reach
        add_portfolio(&a, "IKE");
        assert!(
            sync_at(&a, &shared, None, true, None).unwrap().wrote,
            "creates the missing file"
        );
        assert!(
            !shared.with_extension("ami.tmp").exists(),
            "no temp file left behind"
        );

        add_portfolio(&b, "Konto B");
        let r = sync_at(&b, &shared, None, true, None).unwrap();
        assert_eq!((r.stats.inserted, r.wrote), (1, true));
        assert_eq!(names(&b), ["IKE", "Konto B"]);
        assert!(
            b.data_dir.join(BACKUPS_DIR).join("sync").exists(),
            "safety copy before applying news"
        );

        assert_eq!(
            sync_at(&a, &shared, None, true, None)
                .unwrap()
                .stats
                .inserted,
            1
        );
        assert_eq!(names(&a), ["IKE", "Konto B"]);
        // Everything is in step now: a further round neither writes nor changes.
        let again = sync_at(&b, &shared, None, true, None).unwrap();
        assert_eq!((again.stats, again.wrote), (ImportStats::default(), false));
    }

    #[test]
    fn a_file_that_is_not_ours_is_never_overwritten() {
        let (a, dir) = device();
        add_portfolio(&a, "IKE");
        let other = dir.path().join("notes.ami");
        fs::write(&other, "my shopping list").unwrap();
        assert!(sync_at(&a, &other, None, true, None).is_err());
        assert_eq!(fs::read_to_string(&other).unwrap(), "my shopping list");
    }

    #[test]
    fn an_empty_file_just_created_by_the_user_is_filled() {
        let (a, dir) = device();
        add_portfolio(&a, "IKE");
        let f = dir.path().join("new.ami");
        fs::write(&f, "").unwrap();
        assert!(sync_at(&a, &f, None, true, None).unwrap().wrote);
        assert!(
            fs::read_to_string(&f)
                .unwrap()
                .contains("ami-calculator-export")
        );
    }

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

    #[test]
    fn a_protected_file_on_disk_needs_its_passphrase() {
        let (a, dir) = device();
        let (b, _b_dir) = device();
        let f = dir.path().join("ami-sync.ami");
        add_portfolio(&a, "Tajne IKE");
        assert!(sync_at(&a, &f, Some("pw"), true, None).unwrap().wrote);
        assert!(
            !fs::read_to_string(&f).unwrap().contains("Tajne IKE"),
            "the folder only holds ciphertext"
        );

        let before = fs::read_to_string(&f).unwrap();
        for pw in [None, Some("zle")] {
            assert!(sync_at(&b, &f, pw, true, None).is_err());
        }
        assert_eq!(
            fs::read_to_string(&f).unwrap(),
            before,
            "a refused sync leaves the file alone"
        );
        assert!(names(&b).is_empty());

        assert_eq!(
            sync_at(&b, &f, Some("pw"), true, None)
                .unwrap()
                .stats
                .inserted,
            1
        );
        assert_eq!(names(&b), ["Tajne IKE"]);
    }

    #[test]
    fn only_a_dot_ami_file_may_be_written_and_read() {
        let (a, dir) = device();
        add_portfolio(&a, "IKE");
        for name in ["loot.txt", "no-extension"] {
            let f = dir.path().join(name);
            assert!(sync_at(&a, &f, None, true, None).is_err(), "{name}");
            assert!(!f.exists(), "{name} must not be created");
        }
        assert!(
            sync_at(&a, &dir.path().join("OK.AMI"), None, true, None).is_ok(),
            "case does not matter"
        );
    }

    #[test]
    fn an_oversized_sync_file_is_refused_without_being_read_whole() {
        let (a, dir) = device();
        let f = dir.path().join("big.ami");
        fs::write(&f, vec![b'x'; ami_core::sync::MAX_FILE_BYTES + 1000]).unwrap();
        assert!(sync_at(&a, &f, None, true, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_planted_at_the_temp_name_is_not_written_through() {
        let (a, dir) = device();
        add_portfolio(&a, "IKE");
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, "precious").unwrap();
        let f = dir.path().join("ami-sync.ami");
        std::os::unix::fs::symlink(&victim, dir.path().join("ami-sync.ami.tmp")).unwrap();
        assert!(sync_at(&a, &f, None, true, None).unwrap().wrote);
        assert_eq!(fs::read_to_string(&victim).unwrap(), "precious");
        assert!(
            fs::read_to_string(&f)
                .unwrap()
                .contains("ami-calculator-export")
        );
    }

    #[test]
    fn the_passphrase_of_a_file_on_disk_can_be_changed() {
        let (a, dir) = device();
        let (b, _b_dir) = device();
        let f = dir.path().join("ami-sync.ami");
        add_portfolio(&a, "IKE");
        sync_at(&a, &f, Some("old"), true, None).unwrap();
        assert!(
            sync_at(&a, &f, Some("old"), false, Some("new"))
                .unwrap()
                .wrote
        );
        assert!(
            sync_at(&b, &f, Some("old"), true, None).is_err(),
            "the old one no longer opens it"
        );
        assert_eq!(
            sync_at(&b, &f, Some("new"), true, None)
                .unwrap()
                .stats
                .inserted,
            1
        );
    }
}
