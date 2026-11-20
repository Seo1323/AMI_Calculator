//! Sync through one shared `.ami` file that lives in a folder some tool already syncs
//! (Syncthing, Drive, OneDrive, Nextcloud...). No server: each device merges the file into
//! its database (`Store::import_data`, newer record wins, deletes travel as `deleted_at`)
//! and writes the file back only when it lacks something this device has.
//!
//! shortcut: last-writer-wins per record by `updated_at`, so a device with a wrong clock
//! can win a conflict; upgrade to per-field merge or version vectors if that ever hurts.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::error::CoreError;
use crate::error::Result;
use crate::export::{DataSet, ExportBundle};
use crate::seal;
use crate::store::{Entity, ImportMode, ImportStats, Store};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncOutcome {
    /// What the file had that this device lacked (now applied here).
    pub stats: ImportStats,
    /// New text for the file when it lacks something this device has; `None` = leave it alone.
    pub outgoing: Option<String>,
}

/// Version of every record: "kind|id" -> `updated_at` (manual prices: `fetched_at`).
/// Online prices and exchange rates are left out: every device downloads them itself.
fn versions(data: &DataSet) -> BTreeMap<String, i64> {
    fn add<T: Entity>(out: &mut BTreeMap<String, i64>, kind: &str, items: &[T]) {
        for i in items {
            let m = i.meta();
            out.insert(format!("{kind}|{}", m.id), m.updated_at.timestamp_micros());
        }
    }
    let mut out = BTreeMap::new();
    add(&mut out, "instrument", &data.instruments);
    add(&mut out, "portfolio", &data.portfolios);
    add(&mut out, "target", &data.targets);
    add(&mut out, "trade", &data.trades);
    for q in data.quotes.iter().filter(|q| q.source == "manual") {
        out.insert(
            format!("quote|{}", q.instrument_id),
            q.fetched_at.timestamp_micros(),
        );
    }
    out
}

/// True when `a` holds a record that `b` lacks or has only in an older version.
fn has_news(a: &DataSet, b: &DataSet) -> bool {
    let theirs = versions(b);
    versions(a)
        .iter()
        .any(|(k, v)| theirs.get(k).is_none_or(|t| t < v))
}

/// Merges the file text `incoming` (`None` or blank: there is no file yet) into `store`.
/// With a `passphrase` the file is written sealed (see `seal`); a sealed file needs it to be read.
/// `before_apply` runs just before the store changes, for a safety copy.
/// An unreadable, foreign or undecryptable file is an error and nothing is changed, so it is never
/// overwritten (in particular a sealed file is never replaced by a plain one).
pub fn sync(
    store: &mut Store,
    app_version: &str,
    incoming: Option<&str>,
    passphrase: Option<&str>,
    before_apply: impl FnOnce(&Store) -> Result<()>,
) -> Result<SyncOutcome> {
    let passphrase = passphrase.filter(|p| !p.is_empty());
    let text = incoming.map(str::trim).filter(|s| !s.is_empty());
    let mut was_sealed = false;
    let mut salt = None;
    let theirs = match text {
        Some(t) if seal::is_sealed(t) => {
            was_sealed = true;
            let opened = seal::open(t, passphrase.ok_or(CoreError::PassphraseNeeded)?)?;
            salt = Some(opened.salt);
            Some(ExportBundle::from_json(&opened.plain)?.data)
        }
        Some(t) => Some(ExportBundle::from_json(t)?.data),
        None => None,
    };
    let mut stats = ImportStats::default();
    if let Some(theirs) = &theirs
        && has_news(theirs, &store.export_data()?)
    {
        before_apply(store)?;
        stats = store.import_data(theirs, ImportMode::Merge)?;
    }
    let mut mine = store.export_data()?;
    // A plain file is rewritten sealed as soon as a passphrase is set, even with nothing new.
    let upgrade = passphrase.is_some() && theirs.is_some() && !was_sealed;
    let write = upgrade || theirs.as_ref().is_none_or(|t| has_news(&mine, t));
    let outgoing = if write {
        mine.settings = serde_json::Value::Null; // appearance is per device, never synced
        let json = ExportBundle::new(app_version, mine).to_json()?;
        Some(match passphrase {
            Some(p) => seal::seal(&json, p, salt.as_deref())?,
            None => json,
        })
    } else {
        None
    };
    Ok(SyncOutcome { stats, outgoing })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::CoreError;
    use crate::model::*;
    use crate::money::Currency;
    use rust_decimal_macros::dec;
    use std::{thread::sleep, time::Duration};

    fn portfolio(name: &str) -> Portfolio {
        Portfolio {
            meta: Meta::new(),
            name: name.into(),
            base_currency: Currency::new("PLN").unwrap(),
            tolerance_percent: dec!(5),
            account_type: AccountType::default(),
        }
    }

    fn run(store: &mut Store, file: Option<&str>) -> SyncOutcome {
        sync(store, "test", file, None, |_| Ok(())).unwrap()
    }

    fn tick() {
        sleep(Duration::from_millis(3)); // timestamps have microsecond resolution
    }

    #[test]
    fn a_new_device_gets_everything_and_writes_nothing_back() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        a.save(portfolio("IKE")).unwrap();
        let file = run(&mut a, None)
            .outgoing
            .expect("first sync creates the file");

        let out = run(&mut b, Some(&file));
        assert_eq!(out.stats.inserted, 1);
        assert!(out.outgoing.is_none());
        assert_eq!(b.list::<Portfolio>().unwrap()[0].name, "IKE");
        // Syncing again changes nothing.
        let again = run(&mut b, Some(&file));
        assert_eq!(again.stats, ImportStats::default());
        assert!(again.outgoing.is_none());
    }

    #[test]
    fn edits_and_deletes_travel_both_ways() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        let keep = a.save(portfolio("IKE")).unwrap();
        let drop = a.save(portfolio("Stary")).unwrap();
        let file = run(&mut a, None).outgoing.unwrap();
        run(&mut b, Some(&file));

        tick();
        let mut renamed = b.get::<Portfolio>(keep.meta.id).unwrap().unwrap();
        renamed.name = "IKZE".into();
        b.save(renamed).unwrap();
        b.delete::<Portfolio>(drop.meta.id).unwrap();
        let file = run(&mut b, Some(&file))
            .outgoing
            .expect("b has news for the file");

        let out = run(&mut a, Some(&file));
        assert_eq!(out.stats.updated, 2);
        let left = a.list::<Portfolio>().unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].name, "IKZE");
    }

    #[test]
    fn the_newer_edit_wins_a_conflict_and_nothing_is_lost() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        let p = a.save(portfolio("IKE")).unwrap();
        let first = run(&mut a, None).outgoing.unwrap();
        run(&mut b, Some(&first));
        let extra = b.save(portfolio("Dodany na B")).unwrap();

        tick();
        let mut older = a.get::<Portfolio>(p.meta.id).unwrap().unwrap();
        older.name = "z A".into();
        a.save(older).unwrap();
        tick();
        let mut newer = b.get::<Portfolio>(p.meta.id).unwrap().unwrap();
        newer.name = "z B".into();
        b.save(newer).unwrap();

        let file = run(&mut a, Some(&first)).outgoing.unwrap(); // A writes its edit
        let file = run(&mut b, Some(&file)).outgoing.unwrap(); // B is newer and also adds a record
        run(&mut a, Some(&file));
        for s in [&a, &b] {
            let names: Vec<_> = s
                .list::<Portfolio>()
                .unwrap()
                .into_iter()
                .map(|p| p.name)
                .collect();
            assert_eq!(names.len(), 2);
            assert!(names.contains(&"z B".to_string()), "{names:?}");
            assert!(s.get::<Portfolio>(extra.meta.id).unwrap().is_some());
        }
    }

    #[test]
    fn a_foreign_file_is_refused_and_left_alone() {
        let mut a = Store::open_in_memory().unwrap();
        a.save(portfolio("IKE")).unwrap();
        for text in ["{\"hello\":1}", "not json at all"] {
            let r = sync(&mut a, "test", Some(text), None, |_| Ok(()));
            assert!(matches!(r, Err(CoreError::NotAnExport)), "{text}");
        }
        assert_eq!(a.list::<Portfolio>().unwrap().len(), 1);
    }

    #[test]
    fn a_blank_file_counts_as_no_file_and_the_safety_copy_is_taken_only_for_news() {
        let mut a = Store::open_in_memory().unwrap();
        a.save(portfolio("IKE")).unwrap();
        let mut copies = 0;
        let out = sync(&mut a, "test", Some("  \n"), None, |_| {
            copies += 1;
            Ok(())
        })
        .unwrap();
        assert!(out.outgoing.is_some());
        assert_eq!(copies, 0, "nothing was incoming");

        let file = out.outgoing.unwrap();
        let mut b = Store::open_in_memory().unwrap();
        sync(&mut b, "test", Some(&file), None, |_| {
            copies += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(copies, 1, "the file had news for b");
    }

    #[test]
    fn appearance_settings_stay_on_the_device() {
        let mut a = Store::open_in_memory().unwrap();
        a.save(portfolio("IKE")).unwrap();
        let file = run(&mut a, None).outgoing.unwrap();
        let bundle = ExportBundle::from_json(&file).unwrap();
        assert!(bundle.data.settings.is_null());
    }

    fn run_pw(store: &mut Store, file: Option<&str>, pw: Option<&str>) -> Result<SyncOutcome> {
        sync(store, "test", file, pw, |_| Ok(()))
    }

    #[test]
    fn a_protected_file_travels_between_devices_and_hides_the_data() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        a.save(portfolio("IKE-tajne")).unwrap();
        let file = run_pw(&mut a, None, Some("pw")).unwrap().outgoing.unwrap();
        assert!(seal::is_sealed(&file) && !file.contains("IKE-tajne"));

        assert_eq!(
            run_pw(&mut b, Some(&file), Some("pw"))
                .unwrap()
                .stats
                .inserted,
            1
        );
        assert_eq!(b.list::<Portfolio>().unwrap()[0].name, "IKE-tajne");
        assert!(
            run_pw(&mut b, Some(&file), Some("pw"))
                .unwrap()
                .outgoing
                .is_none(),
            "in step: no rewrite"
        );

        tick();
        b.save(portfolio("Dodany na B")).unwrap();
        let back = run_pw(&mut b, Some(&file), Some("pw"))
            .unwrap()
            .outgoing
            .unwrap();
        assert!(seal::is_sealed(&back));
        assert_eq!(
            run_pw(&mut a, Some(&back), Some("pw"))
                .unwrap()
                .stats
                .inserted,
            1
        );
    }

    #[test]
    fn without_the_right_passphrase_nothing_is_read_changed_or_overwritten() {
        let mut a = Store::open_in_memory().unwrap();
        let mut b = Store::open_in_memory().unwrap();
        a.save(portfolio("IKE")).unwrap();
        b.save(portfolio("Moje B")).unwrap();
        let file = run_pw(&mut a, None, Some("pw")).unwrap().outgoing.unwrap();

        assert!(matches!(
            run_pw(&mut b, Some(&file), None),
            Err(CoreError::PassphraseNeeded)
        ));
        assert!(matches!(
            run_pw(&mut b, Some(&file), Some("")),
            Err(CoreError::PassphraseNeeded)
        ));
        assert!(matches!(
            run_pw(&mut b, Some(&file), Some("nope")),
            Err(CoreError::WrongPassphrase)
        ));
        assert_eq!(b.list::<Portfolio>().unwrap().len(), 1, "b was left alone");
    }

    #[test]
    fn setting_a_passphrase_seals_an_existing_plain_file_even_with_nothing_new() {
        let mut a = Store::open_in_memory().unwrap();
        a.save(portfolio("IKE")).unwrap();
        let plain = run(&mut a, None).outgoing.unwrap();
        assert!(run(&mut a, Some(&plain)).outgoing.is_none());

        let sealed = run_pw(&mut a, Some(&plain), Some("pw"))
            .unwrap()
            .outgoing
            .expect("rewritten sealed");
        assert!(seal::is_sealed(&sealed));
    }
}
