use std::{collections::HashSet, fs, path::PathBuf, sync::Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

const LAYOUT_FILE: &str = "launcher-library-layout.json";
const LAYOUT_SCHEMA: u32 = 1;
const MAX_GROUPS: usize = 256;
const MAX_GAME_IDS: usize = 200_000;

static LAYOUT_IO_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LauncherCollection {
    pub id: String,
    pub name: String,
    pub game_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherShelf {
    pub id: String,
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub collection_id: Option<String>,
    #[serde(default)]
    pub game_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct XmclInstanceGroup {
    pub id: String,
    pub name: String,
    pub game_ids: Vec<String>,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LauncherLibraryLayout {
    pub schema_version: u32,
    pub library_game_ids: Vec<String>,
    pub favorite_game_ids: Vec<String>,
    pub collections: Vec<LauncherCollection>,
    pub shelves: Vec<LauncherShelf>,
    pub xmcl_instance_groups: Vec<XmclInstanceGroup>,
    pub migrated_legacy_at: Option<String>,
    pub updated_at: String,
}

impl Default for LauncherLibraryLayout {
    fn default() -> Self {
        Self {
            schema_version: LAYOUT_SCHEMA,
            library_game_ids: Vec::new(),
            favorite_game_ids: Vec::new(),
            collections: Vec::new(),
            shelves: vec![
                LauncherShelf {
                    id: "recent".to_string(),
                    title: "Recent".to_string(),
                    kind: "recent".to_string(),
                    collection_id: None,
                    game_ids: Vec::new(),
                },
                LauncherShelf {
                    id: "installed".to_string(),
                    title: "Installed".to_string(),
                    kind: "installed".to_string(),
                    collection_id: None,
                    game_ids: Vec::new(),
                },
            ],
            xmcl_instance_groups: vec![XmclInstanceGroup {
                id: "all-instances".to_string(),
                name: "All instances".to_string(),
                game_ids: Vec::new(),
                collapsed: false,
            }],
            migrated_legacy_at: None,
            updated_at: Utc::now().to_rfc3339(),
        }
    }
}

fn layout_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|root| root.join(LAYOUT_FILE))
        .map_err(|error| format!("Cannot resolve launcher library storage: {error}"))
}

fn normalize_game_ids(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| {
            !value.is_empty() && value.len() <= 160 && seen.insert(value.to_ascii_lowercase())
        })
        .take(MAX_GAME_IDS)
        .collect()
}

fn normalize_id(value: String, fallback: &str) -> String {
    let normalized: String = value
        .trim()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
        .take(96)
        .collect();
    if normalized.is_empty() {
        fallback.to_string()
    } else {
        normalized
    }
}

fn sanitize(mut layout: LauncherLibraryLayout) -> LauncherLibraryLayout {
    layout.schema_version = LAYOUT_SCHEMA;
    layout.library_game_ids = normalize_game_ids(layout.library_game_ids);
    layout.favorite_game_ids = normalize_game_ids(layout.favorite_game_ids);
    layout.collections = layout
        .collections
        .into_iter()
        .take(MAX_GROUPS)
        .enumerate()
        .map(|(index, mut collection)| {
            collection.id = normalize_id(collection.id, &format!("collection-{index}"));
            collection.name = collection.name.trim().chars().take(120).collect();
            collection.game_ids = normalize_game_ids(collection.game_ids);
            collection
        })
        .collect();
    layout.shelves = layout
        .shelves
        .into_iter()
        .take(MAX_GROUPS)
        .enumerate()
        .map(|(index, mut shelf)| {
            shelf.id = normalize_id(shelf.id, &format!("shelf-{index}"));
            shelf.title = shelf.title.trim().chars().take(120).collect();
            shelf.kind = shelf.kind.trim().chars().take(40).collect();
            shelf.collection_id = shelf.collection_id.map(|id| normalize_id(id, "collection"));
            shelf.game_ids = normalize_game_ids(shelf.game_ids);
            shelf
        })
        .collect();
    layout.xmcl_instance_groups = layout
        .xmcl_instance_groups
        .into_iter()
        .take(MAX_GROUPS)
        .enumerate()
        .map(|(index, mut group)| {
            group.id = normalize_id(group.id, &format!("instance-group-{index}"));
            group.name = group.name.trim().chars().take(120).collect();
            group.game_ids = normalize_game_ids(group.game_ids);
            group
        })
        .collect();
    layout.updated_at = Utc::now().to_rfc3339();
    layout
}

fn read_unlocked(app: &AppHandle) -> Result<LauncherLibraryLayout, String> {
    let path = layout_path(app)?;
    if !path.exists() {
        return Ok(LauncherLibraryLayout::default());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    let parsed = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid launcher library layout: {error}"))?;
    Ok(sanitize(parsed))
}

#[tauri::command]
pub fn get_launcher_library_layout(app: AppHandle) -> Result<LauncherLibraryLayout, String> {
    let _guard = LAYOUT_IO_LOCK
        .lock()
        .map_err(|_| "Library layout is busy".to_string())?;
    read_unlocked(&app)
}

#[tauri::command]
pub fn save_launcher_library_layout(
    app: AppHandle,
    layout: LauncherLibraryLayout,
) -> Result<LauncherLibraryLayout, String> {
    let _guard = LAYOUT_IO_LOCK
        .lock()
        .map_err(|_| "Library layout is busy".to_string())?;
    let layout = sanitize(layout);
    let bytes = serde_json::to_vec_pretty(&layout)
        .map_err(|error| format!("Cannot serialize launcher library layout: {error}"))?;
    let path = layout_path(&app)?;
    crate::lua_live::atomic_write_path(&path, &bytes)?;
    Ok(layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_deduplicates_and_rejects_empty_game_ids() {
        let mut layout = LauncherLibraryLayout::default();
        layout.library_game_ids =
            vec!["game-a".into(), "GAME-A".into(), "".into(), "game-b".into()];
        let normalized = sanitize(layout);
        assert_eq!(normalized.library_game_ids, vec!["game-a", "game-b"]);
        assert_eq!(normalized.schema_version, LAYOUT_SCHEMA);
    }
}
