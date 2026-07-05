// Provider configuration storage for CloudRedirect

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub provider: Option<String>,
    pub local_path: Option<String>,
    pub authenticated: bool,
    pub last_sync: Option<String>,
    pub tokens: Option<Tokens>,
    pub last_error: Option<String>,
    pub error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
}

/// Get config file path
fn get_config_path() -> Result<PathBuf, String> {
    let app_data = dirs::config_dir()
        .ok_or("Failed to get config directory")?;
    
    let config_dir = app_data.join("0xoLemon");
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }
    
    Ok(config_dir.join("cloud_redirect_config.json"))
}

/// Load provider configuration
pub fn load_config() -> Result<ProviderConfig, String> {
    let config_path = get_config_path()?;
    
    if !config_path.exists() {
        return Ok(ProviderConfig::default());
    }
    
    let content = fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read config: {}", e))?;
    
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse config: {}", e))
}

/// Save provider configuration
pub fn save_config(config: &ProviderConfig) -> Result<(), String> {
    let config_path = get_config_path()?;
    
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;
    
    fs::write(&config_path, json)
        .map_err(|e| format!("Failed to write config: {}", e))?;
    
    Ok(())
}

/// Clear provider configuration
pub fn clear_config() -> Result<(), String> {
    let config_path = get_config_path()?;
    
    if config_path.exists() {
        fs::remove_file(&config_path)
            .map_err(|e| format!("Failed to remove config: {}", e))?;
    }
    
    Ok(())
}
