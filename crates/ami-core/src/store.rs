//! SQLite persistence.
//!
//! Each entity lives in its own table as a JSON document (`data`) next to its
//! metadata columns. Columns used for filtering are SQLite generated columns
//! extracted from the JSON, so the Rust structs stay the single source of
//! truth and adding a field needs no migration. Amounts are serialized as
//! decimal strings, so nothing is lost to floating point.
//!
//! Deletes are soft (`deleted_at`) so a future sync can propagate them.

use std::path::Path;

use chrono::{DateTime, SecondsFormat, SubsecRound, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{CoreError, Result};
use crate::export::DataSet;
use crate::model::*;
use crate::money::MAX_AMOUNT;

pub const UI_SETTINGS_KEY: &str = "ui";

// ---------- Entities ----------

pub trait Entity: Serialize + DeserializeOwned {
    const TABLE: &'static str;
    fn meta(&self) -> &Meta;
    fn meta_mut(&mut self) -> &mut Meta;
    fn validate(&self) -> Result<()> {
        Ok(())
    }
    /// Records (table, id) that must exist before this one can be saved.
    fn references(&self) -> Vec<(&'static str, Uuid)> {
        Vec::new()
    }
}

fn require(ok: bool, msg: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(CoreError::Validation(msg.into()))
    }
}

macro_rules! meta_accessors {
    () => {
        fn meta(&self) -> &Meta {
            &self.meta
        }
        fn meta_mut(&mut self) -> &mut Meta {
            &mut self.meta
        }
    };
}

impl Entity for Instrument {
    const TABLE: &'static str = "instruments";
    meta_accessors!();
    fn validate(&self) -> Result<()> {
        require(!self.symbol.trim().is_empty(), "instrument symbol is empty")?;
        require(!self.name.trim().is_empty(), "instrument name is empty")
    }
}

impl Entity for Portfolio {
    const TABLE: &'static str = "portfolios";
    meta_accessors!();
    fn validate(&self) -> Result<()> {
        require(!self.name.trim().is_empty(), "portfolio name is empty")?;
        require(
            self.tolerance_percent >= rust_decimal::Decimal::ZERO,
            "tolerance must not be negative",
        )
    }
}

impl Entity for AllocationTarget {
    const TABLE: &'static str = "allocation_targets";
    meta_accessors!();
    fn validate(&self) -> Result<()> {
        let p = self.target_percent;
        require(
            p >= rust_decimal::Decimal::ZERO && p <= rust_decimal::Decimal::ONE_HUNDRED,
            "target must be between 0 and 100",
        )
    }
    fn references(&self) -> Vec<(&'static str, Uuid)> {
        vec![
            ("portfolios", self.portfolio_id),
            ("instruments", self.instrument_id),
        ]
    }
}

impl Entity for Trade {
    const TABLE: &'static str = "trades";
    meta_accessors!();
    fn validate(&self) -> Result<()> {
        use rust_decimal::Decimal;
        require(
            self.quantity > Decimal::ZERO,
            "trade quantity must be positive",
        )?;
        require(
            self.price >= Decimal::ZERO,
            "trade price must not be negative",
        )?;
        require(self.fee >= Decimal::ZERO, "trade fee must not be negative")?;
        require(
            self.quantity.max(self.price).max(self.fee) <= MAX_AMOUNT,
            "trade value is too large",
        )
    }
    fn references(&self) -> Vec<(&'static str, Uuid)> {
        vec![
            ("portfolios", self.portfolio_id),
            ("instruments", self.instrument_id),
        ]
    }
}

/// Entity selector for the untyped (JSON) API used by the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntityKind {
    Instrument,
    Portfolio,
    AllocationTarget,
    Trade,
}

macro_rules! with_entity {
    ($kind:expr, $T:ident => $body:expr) => {
        match $kind {
            EntityKind::Instrument => {
                type $T = Instrument;
                $body
            }
            EntityKind::Portfolio => {
                type $T = Portfolio;
                $body
            }
            EntityKind::AllocationTarget => {
                type $T = AllocationTarget;
                $body
            }
            EntityKind::Trade => {
                type $T = Trade;
                $body
            }
        }
    };
}

/// All entity tables, parents before children. The old budget tables (accounts,
/// categories, transactions, budget_limits) stay in existing databases, unused,
/// so an update never deletes anyone's data.
const TABLES: &[&str] = &["instruments", "portfolios", "allocation_targets", "trades"];

// ---------- Schema ----------

const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL CHECK (json_valid(value)),
    updated_at TEXT NOT NULL
);

CREATE TABLE accounts (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data))
);

CREATE TABLE categories (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data))
);

CREATE TABLE transactions (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data)),
    account_id  TEXT GENERATED ALWAYS AS (json_extract(data, '$.accountId')) VIRTUAL,
    category_id TEXT GENERATED ALWAYS AS (json_extract(data, '$.categoryId')) VIRTUAL,
    date        TEXT GENERATED ALWAYS AS (json_extract(data, '$.date')) VIRTUAL
);
CREATE INDEX transactions_account_date ON transactions (account_id, date);
CREATE INDEX transactions_date ON transactions (date);
CREATE INDEX transactions_category ON transactions (category_id);

CREATE TABLE budget_limits (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data)),
    category_id TEXT GENERATED ALWAYS AS (json_extract(data, '$.categoryId')) VIRTUAL,
    month       TEXT GENERATED ALWAYS AS (json_extract(data, '$.month')) VIRTUAL
);
CREATE INDEX budget_limits_month ON budget_limits (month, category_id);

CREATE TABLE instruments (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data))
);

CREATE TABLE portfolios (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data))
);

CREATE TABLE allocation_targets (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data)),
    portfolio_id TEXT GENERATED ALWAYS AS (json_extract(data, '$.portfolioId')) VIRTUAL
);
CREATE INDEX allocation_targets_portfolio ON allocation_targets (portfolio_id);

CREATE TABLE trades (
    id TEXT PRIMARY KEY, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT,
    data TEXT NOT NULL CHECK (json_valid(data)),
    portfolio_id  TEXT GENERATED ALWAYS AS (json_extract(data, '$.portfolioId')) VIRTUAL,
    instrument_id TEXT GENERATED ALWAYS AS (json_extract(data, '$.instrumentId')) VIRTUAL,
    date          TEXT GENERATED ALWAYS AS (json_extract(data, '$.date')) VIRTUAL
);
CREATE INDEX trades_portfolio_date ON trades (portfolio_id, date);
"#,
    r#"
CREATE TABLE quotes (
    instrument_id TEXT PRIMARY KEY,
    data          TEXT NOT NULL CHECK (json_valid(data)),
    fetched_at    TEXT NOT NULL
);
CREATE TABLE fx_rates (
    currency   TEXT PRIMARY KEY,
    data       TEXT NOT NULL CHECK (json_valid(data)),
    fetched_at TEXT NOT NULL
);
"#,
    // Daily portfolio value, written when the app computes a complete valuation. Not part of `.ami`
    // backups: it can only be recorded, never rebuilt.
    r#"
CREATE TABLE value_history (
    portfolio_id TEXT NOT NULL,
    day          TEXT NOT NULL,
    currency     TEXT NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (portfolio_id, day)
);
"#,
];

/// Fixed-width UTC timestamps so string comparison equals time comparison.
fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Micros, true)
}

// ---------- Low-level helpers (work on a Connection or a Transaction) ----------

fn write<T: Entity>(conn: &Connection, item: &T) -> Result<()> {
    let m = item.meta();
    conn.execute(
        &format!(
            "INSERT INTO {} (id, created_at, updated_at, deleted_at, data) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (id) DO UPDATE SET created_at = excluded.created_at, updated_at = excluded.updated_at,
             deleted_at = excluded.deleted_at, data = excluded.data",
            T::TABLE
        ),
        params![m.id.to_string(), ts(m.created_at), ts(m.updated_at), m.deleted_at.map(ts), serde_json::to_string(item)?],
    )?;
    Ok(())
}

fn read<T: Entity>(conn: &Connection, id: Uuid) -> Result<Option<T>> {
    let data: Option<String> = conn
        .query_row(
            &format!("SELECT data FROM {} WHERE id = ?1", T::TABLE),
            [id.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    data.map(|d| serde_json::from_str(&d).map_err(CoreError::from))
        .transpose()
}

fn read_all<T: Entity>(conn: &Connection, include_deleted: bool) -> Result<Vec<T>> {
    let filter = if include_deleted {
        ""
    } else {
        "WHERE deleted_at IS NULL"
    };
    let mut stmt = conn.prepare(&format!(
        "SELECT data FROM {} {filter} ORDER BY created_at, id",
        T::TABLE
    ))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.map(|d| Ok(serde_json::from_str(&d?)?)).collect()
}

/// Records of one portfolio through the indexed `portfolio_id` column (only `trades` and
/// `allocation_targets` have it), instead of parsing every portfolio's records and filtering.
fn read_for_portfolio<T: Entity>(conn: &Connection, portfolio_id: Uuid) -> Result<Vec<T>> {
    let mut stmt = conn.prepare_cached(&format!(
        "SELECT data FROM {} WHERE portfolio_id = ?1 AND deleted_at IS NULL ORDER BY created_at, id",
        T::TABLE
    ))?;
    let rows = stmt.query_map([portfolio_id.to_string()], |r| r.get::<_, String>(0))?;
    rows.map(|d| Ok(serde_json::from_str(&d?)?)).collect()
}

fn set_setting(conn: &Connection, key: &str, value: &Value) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, serde_json::to_string(value)?, ts(Utc::now())],
    )?;
    Ok(())
}

fn write_quote(conn: &Connection, q: &Quote) -> Result<()> {
    conn.execute(
        "INSERT INTO quotes (instrument_id, data, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (instrument_id) DO UPDATE SET data = excluded.data, fetched_at = excluded.fetched_at",
        params![q.instrument_id.to_string(), serde_json::to_string(q)?, ts(q.fetched_at)],
    )?;
    Ok(())
}

fn write_fx(conn: &Connection, r: &FxRate) -> Result<()> {
    conn.execute(
        "INSERT INTO fx_rates (currency, data, fetched_at) VALUES (?1, ?2, ?3)
         ON CONFLICT (currency) DO UPDATE SET data = excluded.data, fetched_at = excluded.fetched_at",
        params![r.currency.code(), serde_json::to_string(r)?, ts(r.fetched_at)],
    )?;
    Ok(())
}

fn read_json_table<T: DeserializeOwned>(conn: &Connection, table: &str) -> Result<Vec<T>> {
    let mut stmt = conn.prepare(&format!("SELECT data FROM {table} ORDER BY 1"))?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    rows.map(|d| Ok(serde_json::from_str(&d?)?)).collect()
}

/// Keeps the newer of the local and incoming value (by `fetched_at`).
fn merge_cached(
    conn: &Connection,
    table: &str,
    key_col: &str,
    key: &str,
    fetched_at: DateTime<Utc>,
) -> Result<bool> {
    let local: Option<String> = conn
        .query_row(
            &format!("SELECT fetched_at FROM {table} WHERE {key_col} = ?1"),
            [key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(local.is_none_or(|l| l < ts(fetched_at)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImportMode {
    /// Wipe local data and load the file.
    Replace,
    /// Keep local data; for records present in both, the newer one wins.
    Merge,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportStats {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
}

fn check_quote(q: &Quote) -> Result<()> {
    let positive = q.price > rust_decimal::Decimal::ZERO;
    require(
        positive && q.price <= MAX_AMOUNT,
        "price must be positive and not absurdly large",
    )
}

fn check_rate(r: &FxRate) -> Result<()> {
    let positive = r.rate_pln > rust_decimal::Decimal::ZERO;
    require(
        positive && r.rate_pln <= MAX_AMOUNT,
        "exchange rate must be positive and not absurdly large",
    )?;
    require(r.currency.code() != "PLN", "PLN is the reference currency")
}

fn import_list<T: Entity>(conn: &Connection, items: &[T], stats: &mut ImportStats) -> Result<()> {
    for item in items {
        item.validate()?; // a hand-edited file must not smuggle in what a normal save would refuse
        match read::<T>(conn, item.meta().id)? {
            None => {
                write(conn, item)?;
                stats.inserted += 1;
            }
            Some(existing) if existing.meta().updated_at < item.meta().updated_at => {
                write(conn, item)?;
                stats.updated += 1;
            }
            Some(_) => stats.skipped += 1,
        }
    }
    Ok(())
}

// ---------- Store ----------

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let mut store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        let latest = MIGRATIONS.len() as i64;
        if version > latest {
            return Err(CoreError::DatabaseTooNew(version));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    fn exists(&self, table: &str, id: Uuid) -> Result<bool> {
        Ok(self.conn.query_row(
            &format!("SELECT EXISTS (SELECT 1 FROM {table} WHERE id = ?1 AND deleted_at IS NULL)"),
            [id.to_string()],
            |r| r.get(0),
        )?)
    }

    // ----- typed API -----

    pub fn get<T: Entity>(&self, id: Uuid) -> Result<Option<T>> {
        Ok(read::<T>(&self.conn, id)?.filter(|t| t.meta().deleted_at.is_none()))
    }

    pub fn list<T: Entity>(&self) -> Result<Vec<T>> {
        read_all(&self.conn, false)
    }

    /// Validates, checks references, stamps timestamps and writes (insert or update).
    pub fn save<T: Entity>(&self, mut item: T) -> Result<T> {
        item.validate()?;
        for (table, id) in item.references() {
            if !self.exists(table, id)? {
                return Err(CoreError::MissingReference {
                    table: table.into(),
                    id: id.to_string(),
                });
            }
        }
        let now = Utc::now().trunc_subsecs(6);
        let created_at =
            read::<T>(&self.conn, item.meta().id)?.map_or(now, |e| e.meta().created_at);
        let meta = item.meta_mut();
        meta.created_at = created_at;
        meta.updated_at = now;
        meta.deleted_at = None;
        write(&self.conn, &item)?;
        Ok(item)
    }

    /// Soft delete. Returns false if there was nothing to delete.
    pub fn delete<T: Entity>(&self, id: Uuid) -> Result<bool> {
        let Some(mut item) = self.get::<T>(id)? else {
            return Ok(false);
        };
        let now = Utc::now().trunc_subsecs(6);
        item.meta_mut().deleted_at = Some(now);
        item.meta_mut().updated_at = now;
        write(&self.conn, &item)?;
        Ok(true)
    }

    // ----- JSON API (for the frontend) -----

    pub fn list_json(&self, kind: EntityKind) -> Result<Vec<Value>> {
        with_entity!(kind, T => self.list::<T>()?.into_iter().map(|t| Ok(serde_json::to_value(t)?)).collect())
    }

    /// Saves a record sent by the UI. A missing `id` means "create".
    pub fn save_json(&self, kind: EntityKind, mut value: Value) -> Result<Value> {
        let obj = value
            .as_object_mut()
            .ok_or_else(|| CoreError::Validation("expected an object".into()))?;
        if !obj.get("id").is_some_and(Value::is_string) {
            obj.insert("id".into(), Value::String(Uuid::now_v7().to_string()));
        }
        let now = serde_json::to_value(Utc::now())?;
        obj.entry("createdAt").or_insert_with(|| now.clone());
        obj.entry("updatedAt").or_insert(now);
        if kind == EntityKind::Trade {
            let trade: Trade =
                serde_json::from_value(value).map_err(|e| CoreError::Validation(e.to_string()))?;
            return Ok(serde_json::to_value(self.save_trade(trade)?)?);
        }
        with_entity!(kind, T => {
            let item: T = serde_json::from_value(value).map_err(|e| CoreError::Validation(e.to_string()))?;
            Ok(serde_json::to_value(self.save(item)?)?)
        })
    }

    pub fn delete_json(&self, kind: EntityKind, id: Uuid) -> Result<bool> {
        if kind == EntityKind::Trade {
            return self.delete_trade(id);
        }
        with_entity!(kind, T => self.delete::<T>(id))
    }

    /// Saves a trade only if the portfolio history stays valid (no overselling).
    pub fn save_trade(&self, trade: Trade) -> Result<Trade> {
        trade.validate()?;
        let mut history: Vec<Trade> = self
            .trades_of(trade.portfolio_id)?
            .into_iter()
            .filter(|t| t.meta.id != trade.meta.id)
            .collect();
        history.push(trade.clone());
        crate::portfolio::holdings(&history)?;
        self.save(trade)
    }

    /// Deletes a trade only if the remaining history stays valid.
    pub fn delete_trade(&self, id: Uuid) -> Result<bool> {
        let Some(trade) = self.get::<Trade>(id)? else {
            return Ok(false);
        };
        let rest: Vec<Trade> = self
            .trades_of(trade.portfolio_id)?
            .into_iter()
            .filter(|t| t.meta.id != id)
            .collect();
        crate::portfolio::holdings(&rest)?;
        self.delete::<Trade>(id)
    }

    // ----- prices and exchange rates -----

    pub fn set_quote(&self, quote: &Quote) -> Result<()> {
        check_quote(quote)?;
        if !self.exists("instruments", quote.instrument_id)? {
            return Err(CoreError::MissingReference {
                table: "instruments".into(),
                id: quote.instrument_id.to_string(),
            });
        }
        write_quote(&self.conn, quote)
    }

    pub fn quotes(&self) -> Result<Vec<Quote>> {
        read_json_table(&self.conn, "quotes")
    }

    pub fn set_fx_rate(&self, rate: &FxRate) -> Result<()> {
        check_rate(rate)?;
        write_fx(&self.conn, rate)
    }

    pub fn fx_rates(&self) -> Result<Vec<FxRate>> {
        read_json_table(&self.conn, "fx_rates")
    }

    /// All known rates as a PLN-based conversion table.
    pub fn fx_table(&self) -> Result<crate::fx::FxTable> {
        let mut table = crate::fx::FxTable::new(crate::money::Currency::new("PLN")?);
        for r in self.fx_rates()? {
            table.set(r.currency, r.rate_pln);
        }
        Ok(table)
    }

    // ----- value history -----

    /// One value per portfolio and day: a later call the same day replaces the earlier one. An
    /// unchanged value writes nothing (a view is computed, and recorded, on every page open).
    pub fn record_value(
        &self,
        portfolio_id: Uuid,
        day: chrono::NaiveDate,
        currency: &crate::money::Currency,
        value: rust_decimal::Decimal,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO value_history (portfolio_id, day, currency, value) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (portfolio_id, day) DO UPDATE SET currency = excluded.currency, value = excluded.value
             WHERE currency IS NOT excluded.currency OR value IS NOT excluded.value",
            params![portfolio_id.to_string(), day.to_string(), currency.code(), value.to_string()],
        )?;
        Ok(())
    }

    /// Recorded values in `currency`, oldest first (days recorded in another base currency are skipped).
    pub fn value_history(
        &self,
        portfolio_id: Uuid,
        currency: &crate::money::Currency,
    ) -> Result<Vec<(chrono::NaiveDate, rust_decimal::Decimal)>> {
        let mut stmt = self.conn.prepare(
            "SELECT day, value FROM value_history WHERE portfolio_id = ?1 AND currency = ?2 ORDER BY day",
        )?;
        let rows = stmt.query_map(params![portfolio_id.to_string(), currency.code()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        rows.map(|r| {
            let (d, v) = r?;
            Ok((
                d.parse()
                    .map_err(|_| CoreError::Validation(format!("bad day {d}")))?,
                v.parse()
                    .map_err(|_| CoreError::Validation(format!("bad value {v}")))?,
            ))
        })
        .collect()
    }

    // ----- portfolio helpers -----

    pub fn trades_of(&self, portfolio_id: Uuid) -> Result<Vec<Trade>> {
        read_for_portfolio(&self.conn, portfolio_id)
    }

    pub fn targets_of(&self, portfolio_id: Uuid) -> Result<Vec<AllocationTarget>> {
        read_for_portfolio(&self.conn, portfolio_id)
    }

    /// Replaces a portfolio's target allocation with the given (instrument, percent) list.
    /// All or nothing: an invalid entry leaves the stored targets untouched.
    pub fn set_targets(
        &self,
        portfolio_id: Uuid,
        targets: &[(Uuid, rust_decimal::Decimal)],
    ) -> Result<()> {
        if self.get::<Portfolio>(portfolio_id)?.is_none() {
            return Err(CoreError::NotFound);
        }
        let distinct: std::collections::HashSet<_> = targets.iter().map(|(id, _)| id).collect();
        require(
            distinct.len() == targets.len(),
            "an instrument appears twice in the targets",
        )?;
        self.in_transaction(|| self.apply_targets(portfolio_id, targets))
    }

    /// Runs `f` as one transaction: all or nothing, and one disk sync instead of one per write
    /// (a quote refresh writes dozens of rows).
    pub fn in_transaction<R>(&self, f: impl FnOnce() -> Result<R>) -> Result<R> {
        self.conn.execute_batch("SAVEPOINT batch")?;
        let result = f();
        let end = if result.is_ok() {
            "RELEASE batch"
        } else {
            "ROLLBACK TO batch; RELEASE batch"
        };
        self.conn.execute_batch(end)?;
        result
    }

    fn apply_targets(
        &self,
        portfolio_id: Uuid,
        targets: &[(Uuid, rust_decimal::Decimal)],
    ) -> Result<()> {
        let existing = self.targets_of(portfolio_id)?;
        for t in &existing {
            if !targets.iter().any(|(id, _)| *id == t.instrument_id) {
                self.delete::<AllocationTarget>(t.meta.id)?;
            }
        }
        for &(instrument_id, target_percent) in targets {
            match existing.iter().find(|t| t.instrument_id == instrument_id) {
                Some(t) if t.target_percent == target_percent => {}
                Some(t) => {
                    self.save(AllocationTarget {
                        target_percent,
                        ..t.clone()
                    })?;
                }
                None => {
                    self.save(AllocationTarget {
                        meta: Meta::new(),
                        portfolio_id,
                        instrument_id,
                        target_percent,
                    })?;
                }
            }
        }
        Ok(())
    }

    // ----- settings -----

    pub fn get_setting(&self, key: &str) -> Result<Option<Value>> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?;
        raw.map(|r| serde_json::from_str(&r).map_err(CoreError::from))
            .transpose()
    }

    pub fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        set_setting(&self.conn, key, value)
    }

    // ----- backup / export / import -----

    /// Everything, including soft-deleted records (so a merge propagates deletes).
    pub fn export_data(&self) -> Result<DataSet> {
        let c = &self.conn;
        Ok(DataSet {
            instruments: read_all(c, true)?,
            portfolios: read_all(c, true)?,
            targets: read_all(c, true)?,
            trades: read_all(c, true)?,
            quotes: read_json_table(c, "quotes")?,
            fx_rates: read_json_table(c, "fx_rates")?,
            settings: self.get_setting(UI_SETTINGS_KEY)?.unwrap_or(Value::Null),
        })
    }

    /// Atomic: either the whole file is imported or nothing changes.
    pub fn import_data(&mut self, data: &DataSet, mode: ImportMode) -> Result<ImportStats> {
        let tx = self.conn.transaction()?;
        if mode == ImportMode::Replace {
            for table in TABLES.iter().rev().chain(&["quotes", "fx_rates"]) {
                tx.execute(&format!("DELETE FROM {table}"), [])?;
            }
            if !data.settings.is_null() {
                set_setting(&tx, UI_SETTINGS_KEY, &data.settings)?;
            }
        }
        let mut stats = ImportStats::default();
        import_list(&tx, &data.instruments, &mut stats)?;
        import_list(&tx, &data.portfolios, &mut stats)?;
        import_list(&tx, &data.targets, &mut stats)?;
        import_list(&tx, &data.trades, &mut stats)?;
        for q in &data.quotes {
            check_quote(q)?;
            if merge_cached(
                &tx,
                "quotes",
                "instrument_id",
                &q.instrument_id.to_string(),
                q.fetched_at,
            )? {
                write_quote(&tx, q)?;
            }
        }
        for r in &data.fx_rates {
            check_rate(r)?;
            if merge_cached(&tx, "fx_rates", "currency", r.currency.code(), r.fetched_at)? {
                write_fx(&tx, r)?;
            }
        }
        tx.commit()?;
        Ok(stats)
    }

    /// Consistent copy of the database file (safe while the app is running).
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let target = path
            .to_str()
            .ok_or_else(|| CoreError::Validation("backup path is not valid UTF-8".into()))?;
        self.conn.execute("VACUUM INTO ?1", [target])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Currency;
    use chrono::NaiveDate;
    use rust_decimal_macros::dec;

    fn portfolio(name: &str) -> Portfolio {
        Portfolio {
            meta: Meta::new(),
            name: name.into(),
            base_currency: Currency::new("PLN").unwrap(),
            tolerance_percent: dec!(5.5),
            account_type: AccountType::default(),
        }
    }

    fn instrument() -> Instrument {
        Instrument {
            meta: Meta::new(),
            symbol: "VWCE.DE".into(),
            name: "Vanguard FTSE All-World".into(),
            kind: InstrumentKind::Etf,
            currency: Currency::new("EUR").unwrap(),
            quote_source: QuoteSource::Yahoo,
            fractional: false,
            interest_percent: None,
            capitalization: None,
            start_date: None,
            maturity_date: None,
        }
    }

    fn trade(portfolio_id: Uuid, instrument_id: Uuid, quantity: rust_decimal::Decimal) -> Trade {
        Trade {
            meta: Meta::new(),
            portfolio_id,
            instrument_id,
            date: NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
            side: TradeSide::Buy,
            quantity,
            price: dec!(100.10),
            fee: dec!(1),
        }
    }

    #[test]
    fn save_get_list_delete() {
        let s = Store::open_in_memory().unwrap();
        let p = s.save(portfolio("IKE")).unwrap();
        let got: Portfolio = s.get(p.meta.id).unwrap().unwrap();
        assert_eq!(got.tolerance_percent, dec!(5.5));
        assert_eq!(s.list::<Portfolio>().unwrap().len(), 1);

        assert!(s.delete::<Portfolio>(p.meta.id).unwrap());
        assert!(s.get::<Portfolio>(p.meta.id).unwrap().is_none());
        assert!(s.list::<Portfolio>().unwrap().is_empty());
        assert!(!s.delete::<Portfolio>(p.meta.id).unwrap());
    }

    #[test]
    fn update_keeps_created_at() {
        let s = Store::open_in_memory().unwrap();
        let a = s.save(portfolio("A")).unwrap();
        let mut b = a.clone();
        b.name = "B".into();
        b.meta.created_at = Utc::now() + chrono::Duration::days(1);
        let b = s.save(b).unwrap();
        assert_eq!(b.meta.created_at, a.meta.created_at);
        assert!(b.meta.updated_at >= a.meta.updated_at);
        assert_eq!(s.list::<Portfolio>().unwrap()[0].name, "B");
    }

    #[test]
    fn validation_and_references() {
        let s = Store::open_in_memory().unwrap();
        assert!(matches!(
            s.save(portfolio("  ")),
            Err(CoreError::Validation(_))
        ));
        let orphan = trade(Uuid::now_v7(), Uuid::now_v7(), dec!(1));
        assert!(matches!(
            s.save(orphan),
            Err(CoreError::MissingReference { .. })
        ));
        let p = s.save(portfolio("A")).unwrap();
        let i = s.save(instrument()).unwrap();
        s.save(trade(p.meta.id, i.meta.id, dec!(1))).unwrap();
        assert!(matches!(
            s.save(trade(p.meta.id, i.meta.id, dec!(0))),
            Err(CoreError::Validation(_))
        ));
        s.delete::<Portfolio>(p.meta.id).unwrap();
        assert!(matches!(
            s.save(trade(p.meta.id, i.meta.id, dec!(1))),
            Err(CoreError::MissingReference { .. })
        ));
    }

    #[test]
    fn json_api_creates_and_validates() {
        let s = Store::open_in_memory().unwrap();
        let v = s
            .save_json(
                EntityKind::Portfolio,
                serde_json::json!({ "name": "IKE", "baseCurrency": "pln", "tolerancePercent": "5.5" }),
            )
            .unwrap();
        assert_eq!(v["baseCurrency"], "PLN");
        assert_eq!(v["tolerancePercent"], "5.5");
        assert!(v["id"].is_string());
        assert_eq!(s.list_json(EntityKind::Portfolio).unwrap().len(), 1);

        let bad = s.save_json(EntityKind::Portfolio, serde_json::json!({ "name": "X" }));
        assert!(matches!(bad, Err(CoreError::Validation(_))));
        let id: Uuid = v["id"].as_str().unwrap().parse().unwrap();
        assert!(s.delete_json(EntityKind::Portfolio, id).unwrap());
    }

    #[test]
    fn settings_roundtrip() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.get_setting(UI_SETTINGS_KEY).unwrap().is_none());
        s.set_setting(UI_SETTINGS_KEY, &serde_json::json!({ "theme": "dark" }))
            .unwrap();
        assert_eq!(
            s.get_setting(UI_SETTINGS_KEY).unwrap().unwrap()["theme"],
            "dark"
        );
    }

    #[test]
    fn export_import_replace() {
        let src = Store::open_in_memory().unwrap();
        let p = src.save(portfolio("A")).unwrap();
        let i = src.save(instrument()).unwrap();
        src.save(trade(p.meta.id, i.meta.id, dec!(12.34))).unwrap();
        src.set_setting(UI_SETTINGS_KEY, &serde_json::json!({ "locale": "en" }))
            .unwrap();
        let data = src.export_data().unwrap();

        let mut dst = Store::open_in_memory().unwrap();
        dst.save(portfolio("local only")).unwrap();
        let stats = dst.import_data(&data, ImportMode::Replace).unwrap();
        assert_eq!(
            stats,
            ImportStats {
                inserted: 3,
                updated: 0,
                skipped: 0
            }
        );
        let portfolios = dst.list::<Portfolio>().unwrap();
        assert_eq!(portfolios.len(), 1);
        assert_eq!(portfolios[0].name, "A");
        assert_eq!(dst.list::<Trade>().unwrap()[0].quantity, dec!(12.34));
        assert_eq!(
            dst.get_setting(UI_SETTINGS_KEY).unwrap().unwrap()["locale"],
            "en"
        );
    }

    #[test]
    fn import_merge_newer_wins() {
        let mut s = Store::open_in_memory().unwrap();
        let a = s.save(portfolio("old")).unwrap();
        let local_only = s.save(portfolio("local")).unwrap();

        let mut newer = a.clone();
        newer.name = "new".into();
        newer.meta.updated_at = a.meta.updated_at + chrono::Duration::seconds(10);
        let mut older = local_only.clone();
        older.name = "stale".into();
        older.meta.updated_at = local_only.meta.updated_at - chrono::Duration::seconds(10);
        let fresh = portfolio("fresh");

        let data = DataSet {
            portfolios: vec![newer, older, fresh],
            ..Default::default()
        };
        let stats = s.import_data(&data, ImportMode::Merge).unwrap();
        assert_eq!(
            stats,
            ImportStats {
                inserted: 1,
                updated: 1,
                skipped: 1
            }
        );
        let mut names: Vec<_> = s
            .list::<Portfolio>()
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        names.sort();
        assert_eq!(names, ["fresh", "local", "new"]);
    }

    #[test]
    fn reopen_file_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ami.db");
        {
            let s = Store::open(&path).unwrap();
            s.save(portfolio("persisted")).unwrap();
            s.backup_to(&dir.path().join("backup.db")).unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), MIGRATIONS.len() as i64);
        assert_eq!(s.list::<Portfolio>().unwrap()[0].name, "persisted");
        let b = Store::open(&dir.path().join("backup.db")).unwrap();
        assert_eq!(b.list::<Portfolio>().unwrap().len(), 1);
    }

    #[test]
    fn every_older_schema_upgrades_and_keeps_user_data() {
        // The unused budget tables must survive too: they stay in users' databases for good.
        for old in 1..MIGRATIONS.len() {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ami.db");
            {
                let conn = Connection::open(&path).unwrap();
                for sql in &MIGRATIONS[..old] {
                    conn.execute_batch(sql).unwrap();
                }
                conn.execute(
                    "INSERT INTO settings (key, value, updated_at) VALUES ('k', '\"v\"', 't')",
                    [],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO accounts (id, created_at, updated_at, data) VALUES ('a', 't', 't', '{}')",
                    [],
                )
                .unwrap();
                conn.pragma_update(None, "user_version", old as i64)
                    .unwrap();
            }
            let s = Store::open(&path).unwrap();
            assert_eq!(
                s.schema_version().unwrap(),
                MIGRATIONS.len() as i64,
                "from v{old}"
            );
            assert_eq!(
                s.get_setting("k").unwrap(),
                Some(serde_json::json!("v")),
                "from v{old}"
            );
            let accounts: i64 = s
                .conn
                .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
                .unwrap();
            assert_eq!(accounts, 1, "budget table lost data from v{old}");
        }
    }

    #[test]
    fn rejects_newer_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ami.db");
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 999)
            .unwrap();
        assert!(matches!(
            Store::open(&path),
            Err(CoreError::DatabaseTooNew(999))
        ));
    }

    #[test]
    fn a_new_trade_needs_a_positive_quantity_and_no_negative_money() {
        let s = Store::open_in_memory().unwrap();
        let p = s.save(portfolio("IKE")).unwrap();
        let i = s.save(instrument()).unwrap();
        let good = trade(p.meta.id, i.meta.id, dec!(1));
        assert!(s.save_trade(good.clone()).is_ok());
        for (what, bad) in [
            (
                "zero quantity",
                Trade {
                    quantity: dec!(0),
                    ..good.clone()
                },
            ),
            (
                "negative quantity",
                Trade {
                    quantity: dec!(-1),
                    ..good.clone()
                },
            ),
            (
                "negative price",
                Trade {
                    price: dec!(-1),
                    ..good.clone()
                },
            ),
            (
                "negative fee",
                Trade {
                    fee: dec!(-1),
                    ..good.clone()
                },
            ),
            (
                "absurdly large price",
                Trade {
                    price: MAX_AMOUNT + dec!(1),
                    ..good.clone()
                },
            ),
        ] {
            let bad = Trade {
                meta: Meta::new(),
                ..bad
            };
            assert!(
                matches!(s.save_trade(bad), Err(CoreError::Validation(_))),
                "{what}"
            );
        }
        // A free (price 0) trade is fine.
        let free = Trade {
            meta: Meta::new(),
            price: dec!(0),
            ..good
        };
        assert!(s.save_trade(free).is_ok());
    }

    #[test]
    fn recording_the_same_value_twice_keeps_one_row_and_a_new_value_replaces_it() {
        let s = Store::open_in_memory().unwrap();
        let p = s.save(portfolio("IKE")).unwrap();
        let pln = Currency::new("PLN").unwrap();
        let day = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        for v in [dec!(100), dec!(100), dec!(120)] {
            s.record_value(p.meta.id, day, &pln, v).unwrap();
        }
        assert_eq!(
            s.value_history(p.meta.id, &pln).unwrap(),
            vec![(day, dec!(120))]
        );
    }
}
