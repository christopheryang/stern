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
    pub secret_key: String,
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
}
