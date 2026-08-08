use serde::{Deserialize, Serialize};

pub const ENGINE_VERSION: &str = "2.6.4";
pub const ENGINE_SOURCE_COMMIT: &str = "9d0dbbf48f349a4172d2d47a936bb41c5f5ecff6";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub version: String,
    pub source_commit: String,
    pub engine_ready: bool,
    pub engine_dir: Option<String>,
    pub steam_path: Option<String>,
    pub steam_running: bool,
    pub steam_process_ids: Vec<u32>,
    pub steam_version: Option<i64>,
    pub steam_version_supported: bool,
    pub supported_steam_versions: Vec<i64>,
    pub dll_installed: bool,
    pub mode: String,
    pub provider: Option<String>,
    pub provider_display_name: Option<String>,
    pub authenticated: bool,
    pub token_path: Option<String>,
    pub sync_path: Option<String>,
    pub account_ids: Vec<String>,
    pub last_error: Option<String>,
    pub supported_providers: Vec<String>,
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SteamRuntimeState {
    pub running: bool,
    pub process_ids: Vec<u32>,
    pub version: Option<i64>,
    pub version_supported: bool,
    pub supported_versions: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigView {
    pub provider: String,
    pub token_path: Option<String>,
    pub sync_path: Option<String>,
    pub authenticated: bool,
    pub upload_inflight_mb: u32,
    pub r2: Option<R2CredentialView>,
    pub s3: Option<S3CredentialView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigInput {
    pub provider: String,
    pub token_path: Option<String>,
    pub sync_path: Option<String>,
    pub upload_inflight_mb: Option<u32>,
    pub r2: Option<R2CredentialsInput>,
    pub s3: Option<S3CredentialsInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct R2CredentialsInput {
    pub account_id: String,
    pub access_key_id: String,
    pub secret_access_key: Option<String>,
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct R2CredentialView {
    pub account_id: String,
    pub access_key_id: String,
    pub has_secret: bool,
    pub bucket: String,
    pub key_prefix: Option<String>,
    pub endpoint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct S3CredentialsInput {
    pub access_key_id: String,
    pub secret_access_key: Option<String>,
    pub bucket: String,
    pub endpoint: String,
    pub region: String,
    pub key_prefix: Option<String>,
    pub sign_payload: bool,
    pub allow_insecure_http: bool,
    pub allow_insecure_tls: bool,
    pub ca_cert_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct S3CredentialView {
    pub access_key_id: String,
    pub has_secret: bool,
    pub bucket: String,
    pub endpoint: String,
    pub region: String,
    pub key_prefix: Option<String>,
    pub sign_payload: bool,
    pub allow_insecure_http: bool,
    pub allow_insecure_tls: bool,
    pub ca_cert_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAppInfo {
    pub account_id: String,
    pub app_id: String,
    pub file_count: u64,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFileInfo {
    pub path: String,
    pub size: u64,
    pub modified_time: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StatsEntry {
    pub account_id: String,
    pub app_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub success: bool,
    pub message: String,
    pub deleted: Option<u64>,
    pub failed: Option<u64>,
    pub raw: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRequest {
    pub source_provider: String,
    pub destination_provider: String,
    pub switch_after_verify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MigrationEvent {
    pub event_type: String,
    pub phase: Option<String>,
    pub message: Option<String>,
    pub file: Option<String>,
    pub done: Option<u64>,
    pub total: Option<u64>,
    pub bytes: Option<u64>,
    pub migrated: Option<u64>,
    pub skipped: Option<u64>,
    pub failed: Option<u64>,
    pub total_bytes: Option<u64>,
}


#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPinConfig {
    pub enabled: bool,
    pub auto_comment: bool,
    pub pinned_apps: Vec<String>,
    pub path: String,
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ManifestPinConfigInput {
    pub enabled: bool,
    pub auto_comment: bool,
    pub pinned_apps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LocalBackupInfo {
    pub id: String,
    pub account_id: String,
    pub app_id: String,
    pub size: u64,
    pub created_at: String,
    pub reason: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    pub id: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub fix_action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    pub generated_at: String,
    pub items: Vec<DiagnosticItem>,
    pub log_tail: Vec<String>,
}
