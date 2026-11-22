use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultStatus {
    pub is_initialized: bool,
    pub is_unlocked: bool,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryDto {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub tags: Vec<String>,
    pub fields: Vec<FieldDto>,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDto {
    pub key: String,
    pub value: String,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEntryRequest {
    pub kind: String,
    pub name: String,
    pub tags: Vec<String>,
    pub fields: Vec<FieldDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateEntryRequest {
    pub id: String,
    pub name: Option<String>,
    pub tags: Option<Vec<String>>,
    pub fields: Option<Vec<FieldDto>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnlockRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateVaultRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendChatRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub message: String,
    pub action: Option<String>,
    pub user_message_display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResponse {
    pub path: String,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRequest {
    pub password: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResponse {
    pub entry_count: usize,
}

/// Status of the downloadable AI model, as reported to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelStatusDto {
    pub model_id: String,
    pub display_name: String,
    /// Plain-language guidance: what the model is good at, what it costs.
    pub description: String,
    /// One of `not_installed`, `installed`, `update_available`.
    pub state: String,
    pub installed_version: Option<String>,
    pub latest_version: String,
    /// Approximate size of the latest model build, in bytes.
    pub size_bytes: u64,
    /// Whether this is the active model (the one chat uses).
    pub is_active: bool,
    /// A download of this model is currently in flight.
    pub is_downloading: bool,
    /// Bytes downloaded so far (0 when not downloading).
    pub downloaded_bytes: u64,
    /// Expected total bytes for the in-flight download.
    pub total_bytes: u64,
}
