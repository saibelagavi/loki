//! DTOs that cross the IPC boundary. None of them carries a secret value or comment text:
//! the frontend sees key names, statuses, counts and short fingerprints only.

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub id: i64,
    pub name: String,
    pub root_path: Option<String>,
    pub description: Option<String>,
    pub target_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TargetDto {
    pub id: i64,
    pub project_id: i64,
    pub branch: String,
    pub stage: String,
    pub label: Option<String>,
    pub color: Option<String>,
    pub is_archived: bool,
    pub current_snapshot_id: Option<i64>,
    pub current_version: Option<i64>,
    pub key_count: i64,
    pub snapshot_count: i64,
    pub last_imported: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDto {
    pub id: i64,
    pub target_id: i64,
    pub version: i64,
    pub note: Option<String>,
    pub source_path: Option<String>,
    pub created_at: String,
    pub key_count: i64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfoDto {
    pub variable_id: i64,
    pub key: String,
    pub is_empty: bool,
    pub has_comment: bool,
    /// First 8 hex characters of the HMAC fingerprint. Equal fingerprint means equal value.
    pub fingerprint: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct KeyNoteDto {
    pub key: String,
    pub description: Option<String>,
    pub required_stages: Vec<String>,
    pub is_secret: bool,
    pub last_rotated: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuditDto {
    pub id: i64,
    pub ts: String,
    pub action: String,
    pub entity: String,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub project_id: i64,
    pub project_name: String,
    pub target_id: i64,
    pub branch: String,
    pub stage: String,
    pub is_empty: bool,
    pub fingerprint: String,
    pub variable_id: i64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchGroup {
    pub key: String,
    pub hits: Vec<SearchHit>,
    /// How many different non-empty values this key has across all hits.
    pub distinct_values: usize,
    pub consistent: bool,
}

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VaultStatus {
    pub exists: bool,
    pub unlocked: bool,
}
