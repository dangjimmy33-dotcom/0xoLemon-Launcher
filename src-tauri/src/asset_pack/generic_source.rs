use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde_json::Value;

use crate::launch::load_source_launch_config;
use crate::manifest::{Catalog, VersionManifest};
use crate::remote_paths::launch_executable_for_game_id;

use super::{
    add_file_asset, add_font_assets, asset_id, default_i18n, mime_for_path, remote_asset_id,
    title_from_slug, AssetPackError, AssetPackManifest, GameAchievement, GameCatalog, GameDetail,
    GameInstallMetadata, GameMedia, GameRating, GameSound, GameSummary, GameVersionInfo, RawAsset,
    SourceAssetBuild,
};

pub(super) fn build_generic_manifest_and_assets(
    source: &Path,
) -> Result<SourceAssetBuild, AssetPackError> {
    let metadata_path = source
        .join("details")
        .join("metadata")
        .join("game-detail.normalized.json");
    let media_manifest_path = source
        .join("details")
        .join("metadata")
        .join("media-manifest.json");
    let metadata: Value =
        read_json_file(&metadata_path).unwrap_or_else(|_| fallback_metadata(source));
    let media_manifest: Value =
        read_json_file(&media_manifest_path).unwrap_or_else(|_| Value::Array(Vec::new()));

    let title = value_string(metadata.get("title")).unwrap_or_else(|| source_title(source));
    let game_id = value_string(metadata.get("gameId"))
        .map(|value| slugify(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| slugify(&title));
    let app_id = value_u64(metadata.get("appId"))
        .or_else(|| {
            std::env::var("OXO_ASSET_APP_ID")
                .ok()
                .and_then(|value| value.parse().ok())
        })
        .unwrap_or_default();
    let install = generic_install_metadata(&game_id, &title);
    let launch = load_source_launch_config(source, &game_id, &install.launch_executable)
        .map_err(AssetPackError::InvalidPack)?;
    let versions = detect_generic_versions(&game_id, app_id);
    let latest_version = versions
        .iter()
        .find(|version| version.latest)
        .or_else(|| versions.last())
        .map(|version| version.version.clone())
        .unwrap_or_else(|| "v1.0".to_string());

    let mut assets = HashMap::new();
    let grid_id = add_prefixed_root_asset(&mut assets, source, &game_id, "grid")?;
    let hero_id = add_prefixed_root_asset(&mut assets, source, &game_id, "hero")?;
    let logo_id = add_prefixed_root_asset(&mut assets, source, &game_id, "logo")?;
    let icon_id = add_prefixed_root_asset(&mut assets, source, &game_id, "icon")?;

    let (media, description_images) =
        add_generic_media_assets(source, &game_id, &media_manifest, &mut assets)?;
    let achievements = add_generic_achievement_assets(source, &game_id, &metadata, &mut assets)?;
    let sounds = add_generic_sound_assets(source, &game_id, &mut assets)?;
    add_font_assets(&mut assets);

    let developers = value_strings(metadata.get("developers"));
    let publishers = value_strings(metadata.get("publishers"));
    let ratings = metadata
        .get("metacritic")
        .and_then(|entry| entry.get("score"))
        .and_then(|value| value_string(Some(value)))
        .map(|score| {
            vec![GameRating {
                source: "Metacritic".to_string(),
                score,
            }]
        })
        .unwrap_or_default();

    let detail = GameDetail {
        game_id: game_id.clone(),
        locale: "en-US".to_string(),
        title: title.clone(),
        short_description: value_string(metadata.get("shortDescription"))
            .unwrap_or_else(|| "Game metadata is stored as remote URLs and can be refreshed with the asset tool.".to_string()),
        detailed_description: rewrite_description_tokens(
            value_string(metadata.get("detailedDescriptionHtml"))
                .or_else(|| value_string(metadata.get("aboutTheGameHtml")))
                .unwrap_or_else(|| "<p>No description available. Run the asset fetch step once to write remote Steam metadata.</p>".to_string()),
            &description_images,
        ),
        developers: if developers.is_empty() {
            vec!["Unknown developer".to_string()]
        } else {
            developers.clone()
        },
        publishers: if publishers.is_empty() {
            if developers.is_empty() {
                vec!["Unknown publisher".to_string()]
            } else {
                developers.clone()
            }
        } else {
            publishers.clone()
        },
        release_date: metadata
            .get("releaseDate")
            .and_then(|value| value.get("date"))
            .and_then(|value| value_string(Some(value)))
            .unwrap_or_default(),
        genres: value_strings(metadata.get("genres")),
        categories: value_strings(metadata.get("categories")),
        ratings,
        media,
        achievements: achievements.clone(),
        sounds,
        install: install.clone(),
        launch: launch.clone(),
        description_images: description_images.values().cloned().collect::<Vec<_>>(),
        versions: versions.clone(),
        metadata_source: value_string(metadata.get("source"))
            .unwrap_or_else(|| "local-generic".to_string()),
    };

    let summary = GameSummary {
        id: game_id.clone(),
        title,
        subtitle: detail
            .developers
            .first()
            .cloned()
            .unwrap_or_else(|| "Game".to_string()),
        developer: detail
            .developers
            .first()
            .cloned()
            .unwrap_or_else(|| "Unknown developer".to_string()),
        publisher: detail
            .publishers
            .first()
            .cloned()
            .unwrap_or_else(|| "Unknown publisher".to_string()),
        latest_version,
        available_versions: versions,
        grid_asset_id: grid_id,
        hero_asset_id: hero_id,
        logo_asset_id: logo_id,
        icon_asset_id: icon_id,
        install,
        launch,
        asset_pack_path: format!("assets/games/{game_id}/core.0xo"),
    };

    let mut details = HashMap::new();
    details.insert(game_id.clone(), detail);
    let manifest = AssetPackManifest {
        generated_at: Utc::now().to_rfc3339(),
        catalog: GameCatalog {
            default_locale: "en-US".to_string(),
            games: vec![summary],
        },
        details,
        assets: HashMap::new(),
        i18n: default_i18n(),
    };

    Ok(SourceAssetBuild {
        game_id,
        manifest,
        assets,
        achievement_count: achievements.len(),
    })
}

fn add_prefixed_root_asset(
    assets: &mut HashMap<String, RawAsset>,
    source: &Path,
    game_id: &str,
    role: &str,
) -> Result<String, AssetPackError> {
    let path = find_prefixed_file(source, role).ok_or_else(|| {
        AssetPackError::AssetNotFound(format!("{role} asset in {}", source.display()))
    })?;
    let id = asset_id(game_id, role);
    add_file_asset(assets, game_id, role, &id, &path)?;
    Ok(id)
}

fn add_generic_media_assets(
    source: &Path,
    game_id: &str,
    manifest: &Value,
    assets: &mut HashMap<String, RawAsset>,
) -> Result<(Vec<GameMedia>, HashMap<String, String>), AssetPackError> {
    let details_root = source.join("details");
    let mut media = Vec::new();
    let mut description_images = HashMap::new();
    let mut video_index = 0_usize;
    let mut screenshot_index = 0_usize;
    let mut description_index = 0_usize;

    for item in manifest.as_array().into_iter().flatten() {
        let role = value_string(item.get("role")).unwrap_or_default();
        let title = value_string(item.get("title")).unwrap_or_else(|| title_from_slug(&role));
        let relative_file = value_string(item.get("file")).unwrap_or_default();
        let source_url = value_string(item.get("sourceUrl"))
            .or_else(|| value_string(item.get("url")))
            .unwrap_or_default();
        let remote_only = item
            .get("remoteOnly")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let local_path = if remote_only || relative_file.is_empty() {
            None
        } else {
            let path = details_root.join(relative_to_path(&relative_file));
            path.exists().then_some(path)
        };

        let make_asset = |assets: &mut HashMap<String, RawAsset>,
                          role_for_asset: &str,
                          logical: &str,
                          local_path: Option<&Path>|
         -> Result<Option<(String, String)>, AssetPackError> {
            if let Some(path) = local_path {
                let asset = asset_id(game_id, logical);
                add_file_asset(assets, game_id, role_for_asset, &asset, path)?;
                return Ok(Some((asset, mime_for_path(path))));
            }
            if !source_url.trim().is_empty() {
                return Ok(Some((
                    remote_asset_id(&source_url),
                    mime_for_remote_url(&source_url, role_for_asset),
                )));
            }
            Ok(None)
        };

        match role.as_str() {
            "video" | "video-preview" | "trailer" | "movie" => {
                let media_id = format!("movie-{video_index:02}");
                if let Some((asset, mime_type)) = make_asset(
                    assets,
                    "video",
                    &format!("media:{media_id}"),
                    local_path.as_deref(),
                )? {
                    media.push(GameMedia {
                        id: media_id,
                        role: "video".to_string(),
                        title,
                        mime_type,
                        asset_id: asset,
                    });
                    video_index += 1;
                }
            }
            "video-thumbnail" | "video-poster" | "video-thumb" | "poster" => {
                let thumb_index = video_index.saturating_sub(1);
                let media_id = format!("movie-thumb-{thumb_index:02}");
                if media.iter().any(|existing| existing.id == media_id) {
                    continue;
                }
                if let Some((asset, mime_type)) = make_asset(
                    assets,
                    "video-thumb",
                    &format!("media:{media_id}"),
                    local_path.as_deref(),
                )? {
                    media.push(GameMedia {
                        id: media_id,
                        role: "video-thumb".to_string(),
                        title,
                        mime_type,
                        asset_id: asset,
                    });
                }
            }
            "screenshot" | "image" => {
                let media_id = format!("screenshot-{screenshot_index:02}");
                if let Some((asset, mime_type)) = make_asset(
                    assets,
                    "screenshot",
                    &format!("media:{media_id}"),
                    local_path.as_deref(),
                )? {
                    media.push(GameMedia {
                        id: media_id,
                        role: "screenshot".to_string(),
                        title,
                        mime_type,
                        asset_id: asset,
                    });
                    screenshot_index += 1;
                }
            }
            "description-image" | "description_image" | "desc-image" => {
                let media_id = format!("desc-img-{description_index:02}");
                if let Some((asset, _mime_type)) = make_asset(
                    assets,
                    "description-image",
                    &media_id,
                    local_path.as_deref(),
                )? {
                    if !relative_file.is_empty() {
                        description_images.insert(relative_file.replace('\\', "/"), asset);
                    } else if !source_url.trim().is_empty() {
                        description_images.insert(source_url.clone(), asset);
                    }
                    description_index += 1;
                }
            }
            _ => {
                let media_id = format!("store-{}-{}", slugify(&role), media.len());
                let _ = make_asset(
                    assets,
                    &role,
                    &format!("media:{media_id}"),
                    local_path.as_deref(),
                )?;
            }
        }
    }

    Ok((media, description_images))
}

fn mime_for_remote_url(url: &str, role: &str) -> String {
    let clean = url.split('?').next().unwrap_or(url).to_ascii_lowercase();
    if role == "video" || clean.ends_with(".mp4") {
        "video/mp4".to_string()
    } else if clean.ends_with(".webm") {
        "video/webm".to_string()
    } else if clean.ends_with(".png") {
        "image/png".to_string()
    } else if clean.ends_with(".webp") {
        "image/webp".to_string()
    } else if clean.ends_with(".avif") {
        "image/avif".to_string()
    } else {
        "image/jpeg".to_string()
    }
}

fn add_generic_achievement_assets(
    source: &Path,
    game_id: &str,
    metadata: &Value,
    assets: &mut HashMap<String, RawAsset>,
) -> Result<Vec<GameAchievement>, AssetPackError> {
    let items = metadata
        .pointer("/achievements/items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut achievements = Vec::with_capacity(items.len());

    for (index, item) in items.iter().enumerate() {
        let raw_id =
            value_string(item.get("apiName")).unwrap_or_else(|| format!("achievement-{index:02}"));
        let id = slugify(&raw_id);
        let icon_file = value_string(item.get("iconFile")).unwrap_or_default();
        let steam_icon = value_string(item.get("steamIcon"))
            .or_else(|| value_string(item.get("icon")))
            .unwrap_or_default();

        let asset = if !icon_file.is_empty() {
            let path = source.join(relative_to_path(&icon_file));
            if path.exists() {
                let asset = asset_id(game_id, &format!("achievement:{id}"));
                add_file_asset(assets, game_id, "achievement", &asset, &path)?;
                asset
            } else if !steam_icon.trim().is_empty() {
                remote_asset_id(&steam_icon)
            } else {
                continue;
            }
        } else if !steam_icon.trim().is_empty() {
            remote_asset_id(&steam_icon)
        } else {
            continue;
        };

        achievements.push(GameAchievement {
            id,
            name: value_string(item.get("displayName")).unwrap_or_else(|| title_from_slug(&raw_id)),
            description: value_string(item.get("description")).unwrap_or_default(),
            icon_asset_id: asset,
            hidden: item.get("hidden").and_then(Value::as_bool).unwrap_or(false),
        });
    }

    Ok(achievements)
}

fn add_generic_sound_assets(
    source: &Path,
    game_id: &str,
    assets: &mut HashMap<String, RawAsset>,
) -> Result<Vec<GameSound>, AssetPackError> {
    let dir = source.join("sounds");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    let mut sounds = Vec::new();
    for entry in entries {
        let path = entry.path();
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "sound".to_string());
        let id = asset_id(game_id, &format!("sound:{stem}"));
        let mime = mime_for_path(&path);
        add_file_asset(assets, game_id, "sound", &id, &path)?;
        sounds.push(GameSound {
            id: stem,
            role: "achievement-unlock".to_string(),
            mime_type: mime,
            asset_id: id,
        });
    }
    Ok(sounds)
}

fn generic_install_metadata(game_id: &str, title: &str) -> GameInstallMetadata {
    let common = format!(r"E:\0xoLemon store\common\{title}");
    let downloading = format!(r"E:\0xoLemon store\downloading\{title}");
    GameInstallMetadata {
        default_store_root: r"E:\0xoLemon store".to_string(),
        default_install_folder: common,
        default_downloading_folder: downloading,
        storage_label: "SSD".to_string(),
        supports_resume: true,
        launch_executable: launch_executable_for_game_id(game_id),
    }
}

fn detect_generic_versions(game_id: &str, app_id: u64) -> Vec<GameVersionInfo> {
    let depot_root = PathBuf::from(r"E:\007Launcher\depot").join(game_id);
    let catalog_path = depot_root.join("catalog.json");
    let mut versions = fs::read(&catalog_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Catalog>(&bytes).ok())
        .map(|catalog| {
            catalog
                .versions
                .iter()
                .map(|entry| {
                    let manifest_path = depot_root.join(relative_to_path(&entry.manifest_path));
                    let size = fs::read(&manifest_path)
                        .ok()
                        .and_then(|bytes| serde_json::from_slice::<VersionManifest>(&bytes).ok())
                        .map(|manifest| manifest.total_size)
                        .unwrap_or(entry.total_size);
                    GameVersionInfo {
                        version: entry.version.clone(),
                        label: entry.version.clone(),
                        build_id: if app_id == 0 {
                            entry.version.trim_start_matches('v').to_string()
                        } else {
                            format!("Steam {app_id}")
                        },
                        size_bytes: size,
                        latest: Some(entry.version.clone())
                            == catalog.effective_latest_version().map(str::to_string),
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if versions.is_empty() {
        versions.push(GameVersionInfo {
            version: "v1.0".to_string(),
            label: "v1.0".to_string(),
            build_id: if app_id == 0 {
                "local".to_string()
            } else {
                format!("Steam {app_id}")
            },
            size_bytes: 0,
            latest: true,
        });
    }

    versions.sort_by(|a, b| a.version.cmp(&b.version));
    versions
}

fn rewrite_description_tokens(html: String, assets: &HashMap<String, String>) -> String {
    assets.iter().fold(html, |text, (file, asset_id)| {
        text.replace(&format!("asset:{file}"), &format!("asset:{asset_id}"))
    })
}

fn find_prefixed_file(source: &Path, prefix: &str) -> Option<PathBuf> {
    let mut entries = fs::read_dir(source)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .starts_with(prefix)
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let stem_len = Path::new(&name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.len())
            .unwrap_or(usize::MAX);
        let ext_rank = match Path::new(&name).extension().and_then(|ext| ext.to_str()) {
            Some("webp") => 0,
            Some("png") => 1,
            Some("jpg") | Some("jpeg") => 2,
            Some("ico") => 3,
            _ => 9,
        };
        (stem_len, ext_rank, name)
    });
    entries.into_iter().next().map(|entry| entry.path())
}

fn relative_to_path(relative: &str) -> PathBuf {
    relative
        .replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect::<PathBuf>()
}

fn source_title(source: &Path) -> String {
    source
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "Game".to_string())
}

fn fallback_metadata(source: &Path) -> Value {
    let title = std::env::var("OXO_ASSET_GAME_NAME").unwrap_or_else(|_| source_title(source));
    let app_id = std::env::var("OXO_ASSET_APP_ID")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or_default();
    serde_json::json!({
        "appId": app_id,
        "title": title,
        "shortDescription": "Remote metadata has not been fetched yet.",
        "detailedDescriptionHtml": "<p>No description available. Use the asset fetch step once to write Steam remote metadata.</p>",
        "developers": [],
        "publishers": [],
        "releaseDate": { "date": "" },
        "genres": [],
        "categories": [],
        "achievements": { "total": 0, "items": [] },
        "source": "fallback-generic"
    })
}

fn read_json_file(path: &Path) -> Result<Value, AssetPackError> {
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(text.trim_start_matches('\u{feff}'))?)
}

fn value_string(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.trim().to_string()).filter(|text| !text.is_empty()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn value_strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                value_string(Some(item))
                    .or_else(|| {
                        item.get("description")
                            .and_then(|value| value_string(Some(value)))
                    })
                    .or_else(|| item.get("name").and_then(|value| value_string(Some(value))))
            })
            .flat_map(split_list_text)
            .collect(),
        Some(Value::String(text)) if !text.trim().is_empty() => {
            split_list_text(text.trim().to_string()).collect()
        }
        _ => Vec::new(),
    }
}

fn split_list_text(text: String) -> impl Iterator<Item = String> {
    text.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .into_iter()
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}
