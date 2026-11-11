//! Runs the push-day rules for a (from, to) pair of targets. Values are decrypted here, inside
//! the core, and never leave it: the result holds key names and messages only.

use crate::core::crypto::VaultKeys;
use crate::core::rules::{self, CheckInput, CheckVar, Finding};
use crate::core::settings::Settings;
use crate::db::models::TargetDto;
use crate::db::repo;
use crate::error::{LokiError, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub findings: Vec<Finding>,
    pub from_label: String,
    pub to_label: String,
    pub to_stage: String,
}

pub fn target_name(t: &TargetDto) -> String {
    match &t.label {
        Some(l) => format!("{} ({}, {})", l, t.branch, t.stage),
        None => format!("{} ({})", t.branch, t.stage),
    }
}

fn check_vars(conn: &Connection, keys: &VaultKeys, snapshot_id: Option<i64>) -> Result<Vec<CheckVar>> {
    let Some(sid) = snapshot_id else { return Ok(vec![]) };
    let fps: HashMap<String, Vec<u8>> = repo::load_meta(conn, sid)?.into_iter().map(|m| (m.key, m.fp)).collect();
    let mut out = Vec::new();
    for e in repo::load_entries(conn, keys, sid)? {
        out.push(CheckVar { key: e.key.clone(), value: e.value.clone(), fp: fps.get(&e.key).cloned().unwrap_or_default(), is_empty: e.value.is_empty() });
    }
    Ok(out)
}

pub fn run(conn: &Connection, keys: &VaultKeys, settings: &Settings, from_id: i64, to_id: i64) -> Result<PushResult> {
    let from = repo::get_target(conn, from_id)?;
    let to = repo::get_target(conn, to_id)?;
    if from.id == to.id {
        return Err(LokiError::Invalid("Pick two different targets.".into()));
    }
    let from_vars = check_vars(conn, keys, from.current_snapshot_id)?;
    let to_vars = check_vars(conn, keys, to.current_snapshot_id)?;
    let notes = repo::notes_map(conn, to.project_id)?;
    let non_prod = repo::non_production_fingerprints(conn, to.project_id)?;
    let disabled: HashSet<String> = settings.disabled_rules.iter().cloned().collect();
    let findings = rules::run(&CheckInput {
        from: &from_vars,
        to: &to_vars,
        to_stage: &to.stage,
        notes: &notes,
        non_prod_fps: &non_prod,
        disabled: &disabled,
        custom: &settings.custom_rules,
    });
    Ok(PushResult { findings, from_label: target_name(&from), to_label: target_name(&to), to_stage: to.stage })
}
