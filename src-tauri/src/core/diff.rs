//! Snapshot diff and compare matrix. Works on fingerprints only: no value is decrypted here.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct VarMeta {
    pub id: i64,
    pub key: String,
    pub position: i64,
    pub fp: Vec<u8>,
    pub is_empty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CellStatus {
    /// This column is the baseline and has the key.
    Base,
    Same,
    Different,
    /// Baseline has the key, this column does not (or baseline column lacks a key others have).
    Missing,
    Empty,
    /// Present here, absent in the baseline.
    Extra,
    /// Absent here and absent in the baseline: nothing to compare.
    None,
}

pub fn cell_status(baseline: Option<&VarMeta>, cell: Option<&VarMeta>, is_baseline_column: bool) -> CellStatus {
    if is_baseline_column {
        return if cell.is_some() { CellStatus::Base } else { CellStatus::Missing };
    }
    match (baseline, cell) {
        (Some(_), None) => CellStatus::Missing,
        (None, None) => CellStatus::None,
        (None, Some(_)) => CellStatus::Extra,
        (Some(b), Some(c)) => {
            if c.is_empty {
                CellStatus::Empty
            } else if b.fp == c.fp {
                CellStatus::Same
            } else {
                CellStatus::Different
            }
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MatrixCell {
    pub status: CellStatus,
    pub variable_id: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatrixRow {
    pub key: String,
    pub cells: Vec<MatrixCell>,
}

/// One column per target (its current snapshot). Rows: baseline keys in file order, then the
/// remaining keys alphabetically.
pub fn compare_matrix(columns: &[Vec<VarMeta>], baseline_col: usize) -> Vec<MatrixRow> {
    let by_key: Vec<HashMap<&str, &VarMeta>> =
        columns.iter().map(|c| c.iter().map(|v| (v.key.as_str(), v)).collect()).collect();

    let mut keys: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Some(base) = columns.get(baseline_col) {
        let mut sorted: Vec<&VarMeta> = base.iter().collect();
        sorted.sort_by_key(|v| v.position);
        for v in sorted {
            if seen.insert(v.key.as_str()) {
                keys.push(v.key.as_str());
            }
        }
    }
    let mut rest: Vec<&str> = columns
        .iter()
        .flat_map(|c| c.iter().map(|v| v.key.as_str()))
        .filter(|k| !seen.contains(k))
        .collect();
    rest.sort_unstable();
    rest.dedup();
    keys.extend(rest);

    keys.into_iter()
        .map(|key| {
            let base = by_key.get(baseline_col).and_then(|m| m.get(key)).copied();
            let cells = by_key
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let cell = m.get(key).copied();
                    MatrixCell {
                        status: cell_status(base, cell, i == baseline_col),
                        variable_id: cell.map(|v| v.id),
                    }
                })
                .collect();
            MatrixRow { key: key.to_string(), cells }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub key: String,
    pub change: Change,
    /// Variable in snapshot `a` (the older side), if the key exists there.
    pub a_variable_id: Option<i64>,
    pub b_variable_id: Option<i64>,
}

/// Diff from `a` to `b`: added in b, removed from a, changed (fingerprint differs).
pub fn snapshot_diff(a: &[VarMeta], b: &[VarMeta]) -> Vec<DiffEntry> {
    let a_map: HashMap<&str, &VarMeta> = a.iter().map(|v| (v.key.as_str(), v)).collect();
    let b_map: HashMap<&str, &VarMeta> = b.iter().map(|v| (v.key.as_str(), v)).collect();
    let mut out = Vec::new();

    let mut b_sorted: Vec<&VarMeta> = b.iter().collect();
    b_sorted.sort_by_key(|v| v.position);
    for v in b_sorted {
        match a_map.get(v.key.as_str()) {
            None => out.push(DiffEntry {
                key: v.key.clone(),
                change: Change::Added,
                a_variable_id: None,
                b_variable_id: Some(v.id),
            }),
            Some(old) if old.fp != v.fp => out.push(DiffEntry {
                key: v.key.clone(),
                change: Change::Changed,
                a_variable_id: Some(old.id),
                b_variable_id: Some(v.id),
            }),
            Some(_) => {}
        }
    }
    let mut a_sorted: Vec<&VarMeta> = a.iter().collect();
    a_sorted.sort_by_key(|v| v.position);
    for v in a_sorted {
        if !b_map.contains_key(v.key.as_str()) {
            out.push(DiffEntry {
                key: v.key.clone(),
                change: Change::Removed,
                a_variable_id: Some(v.id),
                b_variable_id: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(id: i64, key: &str, pos: i64, fp: u8, empty: bool) -> VarMeta {
        VarMeta { id, key: key.into(), position: pos, fp: vec![fp; 32], is_empty: empty }
    }

    #[test]
    fn all_cell_states() {
        let base = var(1, "K", 0, 1, false);
        let same = var(2, "K", 0, 1, false);
        let diff = var(3, "K", 0, 2, false);
        let empty = var(4, "K", 0, 9, true);

        assert_eq!(cell_status(Some(&base), Some(&base), true), CellStatus::Base);
        assert_eq!(cell_status(Some(&base), Some(&same), false), CellStatus::Same);
        assert_eq!(cell_status(Some(&base), Some(&diff), false), CellStatus::Different);
        assert_eq!(cell_status(Some(&base), None, false), CellStatus::Missing);
        assert_eq!(cell_status(Some(&base), Some(&empty), false), CellStatus::Empty);
        assert_eq!(cell_status(None, Some(&same), false), CellStatus::Extra);
        assert_eq!(cell_status(None, None, false), CellStatus::None);
        // Baseline column that lacks a key others have shows as missing.
        assert_eq!(cell_status(None, None, true), CellStatus::Missing);
    }

    #[test]
    fn matrix_rows_and_cells() {
        let baseline = vec![var(1, "B_KEY", 1, 1, false), var(2, "A_KEY", 0, 1, false), var(3, "C_KEY", 2, 1, false)];
        let staging = vec![
            var(10, "A_KEY", 0, 1, false), // same
            var(11, "B_KEY", 1, 5, false), // different
            var(12, "C_KEY", 2, 0, true),  // empty
            var(13, "Z_KEY", 3, 1, false), // extra
        ];
        let rows = compare_matrix(&[baseline, staging], 0);
        let keys: Vec<&str> = rows.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys, ["A_KEY", "B_KEY", "C_KEY", "Z_KEY"]); // baseline order, then the rest
        let st = |r: usize, c: usize| rows[r].cells[c].status;
        assert_eq!((st(0, 0), st(0, 1)), (CellStatus::Base, CellStatus::Same));
        assert_eq!(st(1, 1), CellStatus::Different);
        assert_eq!(st(2, 1), CellStatus::Empty);
        assert_eq!((st(3, 0), st(3, 1)), (CellStatus::Missing, CellStatus::Extra));
        assert_eq!(rows[3].cells[0].variable_id, None);
        assert_eq!(rows[3].cells[1].variable_id, Some(13));
    }

    #[test]
    fn matrix_missing_in_other_column() {
        let baseline = vec![var(1, "ONLY_BASE", 0, 1, false)];
        let other: Vec<VarMeta> = vec![];
        let rows = compare_matrix(&[baseline, other], 0);
        assert_eq!(rows[0].cells[1].status, CellStatus::Missing);
    }

    #[test]
    fn diff_added_removed_changed() {
        let a = vec![var(1, "KEEP", 0, 1, false), var(2, "GONE", 1, 1, false), var(3, "EDIT", 2, 1, false)];
        let b = vec![var(11, "KEEP", 0, 1, false), var(13, "EDIT", 1, 2, false), var(14, "NEW", 2, 1, false)];
        let d = snapshot_diff(&a, &b);
        let find = |k: &str| d.iter().find(|e| e.key == k).map(|e| e.change);
        assert_eq!(find("NEW"), Some(Change::Added));
        assert_eq!(find("GONE"), Some(Change::Removed));
        assert_eq!(find("EDIT"), Some(Change::Changed));
        assert_eq!(find("KEEP"), None);
        assert_eq!(d.len(), 3);
        let edit = d.iter().find(|e| e.key == "EDIT").unwrap();
        assert_eq!((edit.a_variable_id, edit.b_variable_id), (Some(3), Some(13)));
    }
}
