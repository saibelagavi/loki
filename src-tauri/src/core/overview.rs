//! Overview across all projects, built for push day when you look after many projects:
//! - release readiness per project (the push check from staging to production, as counts),
//! - drift: which .env files on disk changed since they were last imported (on demand only),
//! - rotation reminders: secrets and passwords older than the rotation setting.

use crate::core::credentials;
use crate::core::crypto::VaultKeys;
use crate::core::import::sha256;
use crate::core::pushcheck;
use crate::core::rules::Severity;
use crate::core::settings::Settings;
use crate::db::models::TargetDto;
use crate::db::repo;
use crate::error::Result;
use rusqlite::Connection;
use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HealthCheck {
    pub from_target_id: i64,
    pub from_label: String,
    pub to_target_id: i64,
    pub to_label: String,
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHealth {
    pub id: i64,
    pub name: String,
    pub target_count: usize,
    pub stages: Vec<String>,
    pub last_imported: Option<String>,
    pub days_since_import: Option<i64>,
    /// None when the project has no production target with a snapshot, or nothing to compare it with.
    pub check: Option<HealthCheck>,
    pub check_error: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RotationItem {
    /// "key" (a secret key with a last-rotated date in its key note) or "password".
    pub kind: &'static str,
    pub project_id: Option<i64>,
    pub project_name: Option<String>,
    pub name: String,
    pub last_rotated: String,
    pub age_days: i64,
    pub credential_id: Option<i64>,
}

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub projects: usize,
    pub targets: usize,
    pub keys: i64,
    pub passwords: usize,
    pub weak_passwords: usize,
    pub reused_passwords: usize,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub projects: Vec<ProjectHealth>,
    pub rotation: Vec<RotationItem>,
    pub rotation_days: u32,
    pub totals: Totals,
}

/// production as "to"; staging (else the first other stage) as "from". Archived and empty
/// targets are ignored.
pub fn pick_pair(targets: &[TargetDto]) -> Option<(&TargetDto, &TargetDto)> {
    let usable: Vec<&TargetDto> = targets.iter().filter(|t| !t.is_archived && t.current_snapshot_id.is_some()).collect();
    let to = usable.iter().find(|t| t.stage == "production")?;
    let from = usable
        .iter()
        .find(|t| t.stage == "staging")
        .or_else(|| usable.iter().find(|t| t.stage == "preview"))
        .or_else(|| usable.iter().find(|t| t.stage != "production"))?;
    Some((from, to))
}

pub fn build(conn: &Connection, keys: &VaultKeys, settings: &Settings) -> Result<Overview> {
    let mut totals = Totals::default();
    let mut projects = Vec::new();
    for p in repo::list_projects(conn)? {
        let targets = repo::list_targets(conn, p.id)?;
        let active: Vec<&TargetDto> = targets.iter().filter(|t| !t.is_archived).collect();
        totals.projects += 1;
        totals.targets += active.len();
        totals.keys += active.iter().map(|t| t.key_count).sum::<i64>();
        let last_imported = active.iter().filter_map(|t| t.last_imported.clone()).max();
        let days_since_import = match &last_imported {
            Some(ts) => conn.query_row("SELECT CAST(julianday('now') - julianday(?1) AS INTEGER)", [ts], |r| r.get(0)).ok(),
            None => None,
        };
        let mut stages: Vec<String> = active.iter().map(|t| t.stage.clone()).collect();
        stages.sort();
        stages.dedup();
        let (check, check_error) = match pick_pair(&targets) {
            Some((from, to)) => match pushcheck::run(conn, keys, settings, from.id, to.id) {
                Ok(r) => {
                    let n = |s: Severity| r.findings.iter().filter(|f| f.severity == s).count();
                    (
                        Some(HealthCheck {
                            from_target_id: from.id,
                            from_label: r.from_label.clone(),
                            to_target_id: to.id,
                            to_label: r.to_label.clone(),
                            errors: n(Severity::Error),
                            warnings: n(Severity::Warning),
                            info: n(Severity::Info),
                        }),
                        None,
                    )
                }
                Err(e) => (None, Some(e.to_string())),
            },
            None => (None, None),
        };
        projects.push(ProjectHealth { id: p.id, name: p.name, target_count: active.len(), stages, last_imported, days_since_import, check, check_error });
    }

    let days = i64::from(settings.rotation_days);
    let mut rotation = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, kn.key, kn.last_rotated, CAST(julianday('now') - julianday(kn.last_rotated) AS INTEGER) AS age \
             FROM key_notes kn JOIN projects p ON p.id = kn.project_id \
             WHERE kn.is_secret = 1 AND kn.last_rotated IS NOT NULL AND julianday(kn.last_rotated) IS NOT NULL AND age > ?1",
        )?;
        let mut rows = stmt.query([days])?;
        while let Some(r) = rows.next()? {
            rotation.push(RotationItem {
                kind: "key",
                project_id: Some(r.get(0)?),
                project_name: Some(r.get(1)?),
                name: r.get(2)?,
                last_rotated: r.get(3)?,
                age_days: r.get(4)?,
                credential_id: None,
            });
        }
    }
    let creds = credentials::list(conn, keys)?;
    totals.passwords = creds.len();
    totals.weak_passwords = creds.iter().filter(|c| c.is_weak).count();
    totals.reused_passwords = creds.iter().filter(|c| c.is_reused).count();
    for c in creds.into_iter().filter(|c| c.age_days > days) {
        rotation.push(RotationItem {
            kind: "password",
            project_id: c.project_id,
            project_name: c.project_name,
            name: c.title,
            last_rotated: c.password_changed_at,
            age_days: c.age_days,
            credential_id: Some(c.id),
        });
    }
    rotation.sort_by_key(|r| std::cmp::Reverse(r.age_days));
    Ok(Overview { projects, rotation, rotation_days: settings.rotation_days, totals })
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DriftStatus {
    Unchanged,
    Changed,
    Missing,
    Unreadable,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriftRow {
    pub project_id: i64,
    pub project_name: String,
    pub target_id: i64,
    pub branch: String,
    pub stage: String,
    pub path: String,
    pub status: DriftStatus,
    pub last_imported: Option<String>,
}

/// Re-reads each target's source file and compares its SHA-256 with the current snapshot.
/// Runs only when the user asks (never at startup). Reads files; writes nothing.
pub fn drift(conn: &Connection) -> Result<Vec<DriftRow>> {
    let mut out = Vec::new();
    for p in repo::list_projects(conn)? {
        for t in repo::list_targets(conn, p.id)?.into_iter().filter(|t| !t.is_archived) {
            let Some(src) = repo::last_source(conn, t.id)? else { continue };
            let path = std::path::Path::new(&src.path);
            let status = if !path.exists() {
                DriftStatus::Missing
            } else {
                match std::fs::read(path) {
                    Err(_) => DriftStatus::Unreadable,
                    Ok(bytes) => {
                        let hash = sha256(&bytes);
                        if src.current_sha.as_deref() == Some(hash.as_slice()) {
                            DriftStatus::Unchanged
                        } else {
                            DriftStatus::Changed
                        }
                    }
                }
            };
            out.push(DriftRow {
                project_id: p.id,
                project_name: p.name.clone(),
                target_id: t.id,
                branch: t.branch,
                stage: t.stage,
                path: src.path,
                status,
                last_imported: t.last_imported,
            });
        }
    }
    // Problems first.
    out.sort_by_key(|r| (r.status == DriftStatus::Unchanged, r.project_name.to_lowercase()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::crypto::{derive_keys, TEST_PARAMS};
    use crate::core::parser::parse;
    use crate::db::migrations::open_memory;

    fn setup() -> (Connection, VaultKeys) {
        (open_memory(), derive_keys(b"fake_password_1", &[6u8; 16], TEST_PARAMS).unwrap())
    }

    #[test]
    fn readiness_runs_staging_to_production_per_project() {
        let (mut conn, k) = setup();
        let p = repo::create_project(&mut conn, "API", None, None).unwrap();
        let prod = repo::create_target(&mut conn, p.id, "main", "production", None, None).unwrap();
        let stg = repo::create_target(&mut conn, p.id, "staging", "staging", None, None).unwrap();
        repo::create_snapshot(&mut conn, &k, prod.id, &parse("A=fake_value_1\nDEBUG=true\n").entries, None, None, None, "import").unwrap();
        repo::create_snapshot(&mut conn, &k, stg.id, &parse("A=fake_value_1\nNEW=fake_value_2\n").entries, None, None, None, "import").unwrap();
        // A project with no production target is listed without a check.
        repo::create_project(&mut conn, "Web", None, None).unwrap();

        let o = build(&conn, &k, &Settings::default()).unwrap();
        assert_eq!(o.totals.projects, 2);
        let api = o.projects.iter().find(|x| x.name == "API").unwrap();
        let c = api.check.as_ref().unwrap();
        assert_eq!((c.from_target_id, c.to_target_id), (stg.id, prod.id));
        assert_eq!(c.errors, 1, "NEW missing in production");
        assert!(c.warnings >= 1, "DEBUG=true in production");
        assert_eq!(api.days_since_import, Some(0));
        assert!(o.projects.iter().find(|x| x.name == "Web").unwrap().check.is_none());
        let json = serde_json::to_string(&o).unwrap();
        assert!(!json.contains("fake_value"));
    }

    #[test]
    fn rotation_reminders_for_old_secrets_and_passwords() {
        let (mut conn, k) = setup();
        let p = repo::create_project(&mut conn, "API", None, None).unwrap();
        repo::set_key_note(&mut conn, p.id, "OLD_SECRET", None, vec![], true, Some("2020-01-01".into())).unwrap();
        repo::set_key_note(&mut conn, p.id, "FRESH", None, vec![], true, Some("2099-01-01".into())).unwrap();
        repo::set_key_note(&mut conn, p.id, "NOT_SECRET", None, vec![], false, Some("2020-01-01".into())).unwrap();
        let c = credentials::create(
            &mut conn,
            &k,
            &credentials::CredentialInput { title: "Old DB".into(), username: String::new(), url: String::new(), tags: vec![], project_id: None, password: Some("x".into()), notes: None },
        )
        .unwrap();
        conn.execute("UPDATE credentials SET password_changed_at = '2020-01-01T00:00:00Z' WHERE id = ?1", [c.id]).unwrap();
        let o = build(&conn, &k, &Settings::default()).unwrap();
        let names: Vec<&str> = o.rotation.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names.contains(&"OLD_SECRET") && names.contains(&"Old DB"));
        assert_eq!(o.totals.weak_passwords, 1);
    }

    #[test]
    fn pair_picking() {
        let t = |id: i64, stage: &str, snap: bool, archived: bool| TargetDto {
            id,
            project_id: 1,
            branch: "b".into(),
            stage: stage.into(),
            label: None,
            color: None,
            is_archived: archived,
            current_snapshot_id: snap.then_some(id),
            current_version: None,
            key_count: 0,
            snapshot_count: 0,
            last_imported: None,
        };
        let ts = vec![t(1, "dev", true, false), t(2, "production", true, false), t(3, "staging", true, true), t(4, "preview", true, false)];
        let (from, to) = pick_pair(&ts).unwrap();
        assert_eq!((from.id, to.id), (4, 2), "archived staging skipped, preview preferred over dev");
        assert!(pick_pair(&[t(1, "staging", true, false)]).is_none());
        assert!(pick_pair(&[t(1, "production", true, false), t(2, "staging", false, false)]).is_none());
    }

    #[test]
    fn drift_detects_changed_missing_and_unchanged_files() {
        let (mut conn, k) = setup();
        let mut dir = std::env::temp_dir();
        dir.push(format!("loki-drift-{}-{}", std::process::id(), crate::core::crypto::to_hex(&crate::core::crypto::random_bytes::<4>().unwrap())));
        dir.push("My Projects");
        std::fs::create_dir_all(&dir).unwrap();
        let p = repo::create_project(&mut conn, "API", None, None).unwrap();
        let mut ids = vec![];
        for (name, stage) in [(".env.production", "production"), (".env.staging", "staging"), (".env", "local")] {
            let path = dir.join(name);
            std::fs::write(&path, "A=fake_value_1\n").unwrap();
            let t = repo::create_target(&mut conn, p.id, "main", stage, None, None).unwrap();
            let src = crate::core::import::resolve(Some(path.to_string_lossy().to_string()), None).unwrap();
            repo::create_snapshot(&mut conn, &k, t.id, &src.parsed.entries, None, src.path.as_deref(), src.sha256.as_ref().map(|h| h.as_slice()), "import").unwrap();
            ids.push(t.id);
        }
        std::fs::write(dir.join(".env.staging"), "A=fake_value_2\n").unwrap();
        std::fs::remove_file(dir.join(".env")).unwrap();
        let rows = drift(&conn).unwrap();
        let status = |id: i64| rows.iter().find(|r| r.target_id == id).unwrap().status;
        assert_eq!(status(ids[0]), DriftStatus::Unchanged);
        assert_eq!(status(ids[1]), DriftStatus::Changed);
        assert_eq!(status(ids[2]), DriftStatus::Missing);
        assert_eq!(rows.last().unwrap().status, DriftStatus::Unchanged, "problems sorted first");
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }
}
