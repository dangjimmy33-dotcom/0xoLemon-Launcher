use tauri::AppHandle;

pub use crate::lightning_integration::{
    LightningCatalogResponse as GameToolsCatalogResponse,
    LightningExecutable as GameToolsExecutable, LightningGameStatus as GameToolsGameStatus,
    LightningIntegrationStatus as GameToolsStatus,
    LightningPackageRequest as GameToolsPackageRequest,
    LightningPackageResult as GameToolsPackageResult,
    LightningSourceIdentity as GameToolsSourceIdentity,
};

#[tauri::command]
pub fn get_game_tools_catalog(kind: String) -> Result<GameToolsCatalogResponse, String> {
    crate::lightning_integration::get_lightning_catalog(kind)
}

#[tauri::command]
pub fn get_game_tools_status() -> Result<GameToolsStatus, String> {
    crate::lightning_integration::get_lightning_integration_status()
}

#[tauri::command]
pub async fn import_game_tools_files(
    app: AppHandle,
    paths: Option<Vec<String>>,
) -> Result<Option<crate::game_tools_import::GameToolsImportResult>, String> {
    crate::game_tools_import::import_files(app, paths).await
}

#[tauri::command]
pub async fn get_game_tools_package_identity(
    kind: String,
    app_id: u32,
) -> Result<GameToolsSourceIdentity, String> {
    crate::lightning_integration::get_lightning_package_identity(kind, app_id).await
}

#[tauri::command]
pub async fn pick_game_tools_install_dir(
    app: AppHandle,
    kind: String,
    app_id: u32,
) -> Result<Option<String>, String> {
    crate::lightning_integration::pick_lightning_install_dir(app, kind, app_id).await
}

#[tauri::command]
pub async fn apply_game_tools_package(
    app: AppHandle,
    request: GameToolsPackageRequest,
) -> Result<GameToolsPackageResult, String> {
    crate::lightning_integration::apply_lightning_package(app, request).await
}

#[tauri::command]
pub async fn restore_latest_game_tools_package(
    app: AppHandle,
    app_id: u32,
) -> Result<GameToolsPackageResult, String> {
    crate::lightning_integration::restore_latest_lightning_package(app, app_id).await
}

#[tauri::command]
pub fn get_game_tools_game_status(app_id: u32) -> Result<GameToolsGameStatus, String> {
    crate::lightning_integration::get_lightning_game_status(app_id)
}

#[tauri::command]
pub async fn list_game_tools_executables(app_id: u32) -> Result<Vec<GameToolsExecutable>, String> {
    crate::lightning_integration::list_lightning_game_executables(app_id).await
}

#[tauri::command]
pub async fn apply_game_tools_steamless(
    app_id: u32,
    relative_executable: String,
) -> Result<crate::steamless::SteamlessResult, String> {
    crate::lightning_integration::apply_lightning_steamless(app_id, relative_executable).await
}

#[tauri::command]
pub async fn restore_game_tools_steamless(
    app_id: u32,
    relative_executable: String,
) -> Result<String, String> {
    crate::lightning_integration::restore_lightning_steamless(app_id, relative_executable).await
}
