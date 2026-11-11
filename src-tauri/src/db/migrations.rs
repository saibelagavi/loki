//! Schema migrations, tracked with `PRAGMA user_version`. Each migration runs in a transaction.
//!
//! Data safety: before migrating an EXISTING vault to a newer schema, a consistent copy of the
//! whole database is written with `VACUUM INTO` to `<data dir>/backups/`. Migrations are additive
//! (new tables only), and each one runs in a transaction, so a failure leaves the vault as it was.

use crate::error::{LokiError, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const MIGRATIONS: &[&str] = &[include_str!("schema.sql"), include_str!("schema_v2.sql")];

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    Ok(conn)
}

pub fn configure(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA secure_delete = ON;")?;
    Ok(())
}

pub fn version(conn: &Connection) -> Result<usize> {
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    Ok(v.max(0) as usize)
}

/// Brings the schema up to date. If the vault already has data at an older schema and
/// `backup_dir` is given, a full copy is written there first. Returns the backup path, if any.
pub fn migrate_with_backup(conn: &mut Connection, backup_dir: Option<&Path>) -> Result<Option<PathBuf>> {
    let current = version(conn)?;
    let mut backup = None;
    if current > 0 && current < MIGRATIONS.len() {
        if let Some(dir) = backup_dir {
            backup = Some(backup_copy(conn, dir, &format!("before-schema-v{}", MIGRATIONS.len()))?);
        }
    }
    migrate_to(conn, MIGRATIONS.len())?;
    Ok(backup)
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    migrate_to(conn, MIGRATIONS.len())
}

pub fn migrate_to(conn: &mut Connection, target: usize) -> Result<()> {
    let current = version(conn)?;
    for (i, sql) in MIGRATIONS.iter().enumerate().take(target).skip(current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
    }
    Ok(())
}

/// Writes a consistent copy of the open database to `dir/loki-<tag>-<timestamp>.db`.
pub fn backup_copy(conn: &Connection, dir: &Path, tag: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp: String = conn.query_row("SELECT strftime('%Y%m%d-%H%M%S','now')", [], |r| r.get(0))?;
    let mut path = dir.join(format!("loki-{tag}-{stamp}.db"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("loki-{tag}-{stamp}-{n}.db"));
        n += 1;
    }
    let target = path.to_str().ok_or_else(|| LokiError::Io("The backup path is not valid text.".into()))?;
    conn.execute("VACUUM INTO ?1", [target])?;
    Ok(path)
}

#[cfg(test)]
pub fn open_memory() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    configure(&conn).unwrap();
    migrate(&mut conn).unwrap();
    conn
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("loki-mig-{tag}-{}-{}", std::process::id(), crate::core::crypto::to_hex(&crate::core::crypto::random_bytes::<4>().unwrap())));
        p.push("My Data");
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn migrates_once_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), MIGRATIONS.len());
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name IN ('variables', 'credentials')", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn upgrading_an_old_vault_keeps_every_row_and_writes_a_backup_first() {
        let dir = temp_dir("upgrade");
        let db = dir.join("loki.db");
        {
            // A vault made by the previous release (schema v1) with some rows in it.
            let mut conn = open(&db).unwrap();
            migrate_to(&mut conn, 1).unwrap();
            conn.execute_batch(
                "INSERT INTO projects (name, created_at, updated_at) VALUES ('Kept', 'x', 'x');
                 INSERT INTO targets (project_id, branch, stage) VALUES (1, 'main', 'production');
                 INSERT INTO snapshots (target_id, version, created_at) VALUES (1, 1, 'x');
                 INSERT INTO variables (snapshot_id, key, position, value_nonce, value_ct, fingerprint, is_empty) VALUES (1, 'A', 0, x'00', x'00', x'00', 0);
                 INSERT INTO audit_log (ts, action, entity) VALUES ('x', 'import', 'target:1');",
            )
            .unwrap();
            assert_eq!(version(&conn).unwrap(), 1);
        }
        let mut conn = open(&db).unwrap();
        let backups = dir.join("backups");
        let made = migrate_with_backup(&mut conn, Some(&backups)).unwrap().expect("a backup is written before upgrading");
        assert_eq!(version(&conn).unwrap(), MIGRATIONS.len());
        for (table, rows) in [("projects", 1), ("targets", 1), ("snapshots", 1), ("variables", 1), ("audit_log", 1), ("credentials", 0)] {
            let n: i64 = conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(n, rows, "{table}");
        }
        // The backup is a complete, openable v1 database.
        let b = Connection::open(&made).unwrap();
        assert_eq!(version(&b).unwrap(), 1);
        let n: i64 = b.query_row("SELECT COUNT(*) FROM variables", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
        drop(b);
        // Already up to date: no second backup.
        assert!(migrate_with_backup(&mut conn, Some(&backups)).unwrap().is_none());
        drop(conn);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn a_new_vault_needs_no_backup() {
        let dir = temp_dir("fresh");
        let mut conn = open(&dir.join("loki.db")).unwrap();
        assert!(migrate_with_backup(&mut conn, Some(&dir.join("backups"))).unwrap().is_none());
        assert!(!dir.join("backups").exists());
        drop(conn);
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    /// Dry-run of the upgrade against a COPY of a real vault, without needing its password:
    /// `LOKI_MIGRATION_TEST_DB=<path to a copy> cargo test real_vault_copy -- --ignored`
    #[test]
    #[ignore]
    fn real_vault_copy_upgrades_without_losing_rows() {
        let Some(path) = std::env::var_os("LOKI_MIGRATION_TEST_DB") else { return };
        let path = PathBuf::from(path);
        let tables = ["vault_meta", "projects", "targets", "snapshots", "variables", "key_notes", "audit_log", "settings"];
        let count = |c: &Connection, t: &str| -> i64 { c.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0)).unwrap() };
        let mut conn = open(&path).unwrap();
        let before: Vec<i64> = tables.iter().map(|t| count(&conn, t)).collect();
        let from = version(&conn).unwrap();
        let backup = migrate_with_backup(&mut conn, Some(&path.parent().unwrap().join("backups"))).unwrap();
        let after: Vec<i64> = tables.iter().map(|t| count(&conn, t)).collect();
        assert_eq!(before, after);
        let ok: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        assert_eq!(ok, "ok");
        // Row counts only; no values are read or printed.
        let summary: Vec<String> = tables.iter().zip(&after).map(|(t, n)| format!("{t}={n}")).collect();
        let report = format!("schema v{from} -> v{}; rows unchanged: {}; backup: {:?}", version(&conn).unwrap(), summary.join(", "), backup);
        std::fs::write(path.with_extension("migration-report.txt"), report).unwrap();
    }
}
