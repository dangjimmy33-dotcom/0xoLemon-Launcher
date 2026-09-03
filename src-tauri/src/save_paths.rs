/// Version-aware save game path mapping for all games.
///
/// Each entry maps a game_id to a list of `VersionedSavePath`.
/// When a game exits, the launcher resolves the correct save path for the
/// installed version and copies those files to a local snapshot directory.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum SaveProviderKind {
    Gse,
    GoldbergSteamEmu,
    GoldbergUplayEmu,
    LegacyVersioned,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedSaveRoot {
    #[serde(default)]
    pub index: usize,
    pub provider: SaveProviderKind,
    pub provider_save_id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct VersionedSavePath {
    /// Minimum version (inclusive, semver-like string).  None = any.
    pub version_from: Option<&'static str>,
    /// Maximum version (inclusive). None = open-ended (all newer versions).
    pub version_to: Option<&'static str>,
    /// Path template — supports the same variables as the cloud-save expansion:
    ///   {appData}      → %APPDATA%
    ///   {localAppData} → %LOCALAPPDATA%
    ///   {userProfile}  → %USERPROFILE%
    ///   {systemDrive}  → %SystemDrive%  (usually "C:")
    ///   {documents}    → %USERPROFILE%\Documents
    pub path_template: &'static str,
}

/// Return the static save-path rules for every game the launcher knows about.
pub fn all_save_path_rules() -> &'static [(&'static str, &'static [VersionedSavePath])] {
    use std::sync::OnceLock;
    static RULES: OnceLock<Vec<(&'static str, &'static [VersionedSavePath])>> = OnceLock::new();
    RULES.get_or_init(|| vec![("007-first-light", RULES_007_FIRST_LIGHT)])
}

static RULES_007_FIRST_LIGHT: &[VersionedSavePath] = &[
    // v1.0.0 and v1.0.1 use the GoldBerg Steam Emulator save dir
    VersionedSavePath {
        version_from: Some("v1.0.0"),
        version_to: Some("v1.0.1"),
        path_template: "{appData}\\GSE Saves\\3768760\\remote",
    },
    // v1.0.2 – v1.0.4: save path unknown, skip backup
    // v1.1.0+ uses the RUNE/Steam layout under Public Documents
    VersionedSavePath {
        version_from: Some("v1.1.0"),
        version_to: None,
        path_template: "{systemDrive}\\Users\\Public\\Documents\\Steam\\RUNE\\3768760",
    },
];

/// Compare two version strings of the form "vX.Y.Z" lexicographically.
/// Returns true when `version` falls within [from, to].
pub fn version_in_range(version: &str, from: Option<&str>, to: Option<&str>) -> bool {
    if let Some(f) = from {
        if compare_versions(version, f) < 0 {
            return false;
        }
    }
    if let Some(t) = to {
        if compare_versions(version, t) > 0 {
            return false;
        }
    }
    true
}

/// Simple numeric version comparison for strings like "v1.0.4".
/// Strips leading 'v', splits on '.', compares component by component.
/// Returns -1 / 0 / 1.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.parse::<u64>().ok())
            .collect()
    };
    let av = parse(a);
    let bv = parse(b);
    let len = av.len().max(bv.len());
    for i in 0..len {
        let av_i = av.get(i).copied().unwrap_or(0);
        let bv_i = bv.get(i).copied().unwrap_or(0);
        if av_i < bv_i {
            return -1;
        }
        if av_i > bv_i {
            return 1;
        }
    }
    0
}

/// Expand a path template to an absolute PathBuf.
pub fn expand_save_template(template: &str) -> PathBuf {
    let app_data = std::env::var("APPDATA").unwrap_or_default();
    expand_save_template_with_app_data(template, Path::new(&app_data))
}

fn expand_save_template_with_app_data(template: &str, app_data: &Path) -> PathBuf {
    let app_data = app_data.to_string_lossy();
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
    let system_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());

    let expanded = template
        .replace("{appData}", &app_data)
        .replace("{localAppData}", &local_app_data)
        .replace("{userProfile}", &user_profile)
        .replace("{documents}", &format!("{user_profile}\\Documents"))
        .replace("{systemDrive}", &system_drive);

    PathBuf::from(expanded)
}

/// Find all save directories that apply to `game_id` at `installed_version`.
/// Returns an empty vec when the version is unknown / no rules match.
pub fn resolve_save_paths(game_id: &str, installed_version: &str) -> Vec<PathBuf> {
    let rules = all_save_path_rules();
    let game_rules = rules
        .iter()
        .find(|(id, _)| *id == game_id)
        .map(|(_, r)| *r)
        .unwrap_or(&[]);

    game_rules
        .iter()
        .filter(|rule| version_in_range(installed_version, rule.version_from, rule.version_to))
        .map(|rule| expand_save_template(rule.path_template))
        .collect()
}

/// Resolve every narrowly-scoped save root supported by SaveProtection.
///
/// Provider roots are resolved only from the pinned local runtime catalog.
/// An AppID or a game name by itself never opts a game into save protection.
pub fn resolve_save_roots(
    game_id: &str,
    installed_version: &str,
    _app_id: Option<u32>,
) -> Vec<ResolvedSaveRoot> {
    let app_data = std::env::var_os("APPDATA").map(PathBuf::from);
    resolve_save_roots_with_app_data(game_id, installed_version, app_data.as_deref())
}

fn resolve_save_roots_with_app_data(
    game_id: &str,
    installed_version: &str,
    app_data: Option<&Path>,
) -> Vec<ResolvedSaveRoot> {
    let mut roots = Vec::new();

    for provider in crate::managed_game_runtime::save_providers_for_game(game_id) {
        match provider.provider {
            SaveProviderKind::Gse
            | SaveProviderKind::GoldbergSteamEmu
            | SaveProviderKind::GoldbergUplayEmu => {
                let Some(app_data) = app_data.filter(|path| !path.as_os_str().is_empty()) else {
                    continue;
                };
                let directory = match provider.provider {
                    SaveProviderKind::Gse => "GSE Saves",
                    SaveProviderKind::GoldbergSteamEmu => "Goldberg SteamEmu Saves",
                    SaveProviderKind::GoldbergUplayEmu => "Goldberg UplayEmu Saves",
                    SaveProviderKind::LegacyVersioned => unreachable!(),
                };
                roots.push(ResolvedSaveRoot {
                    index: 0,
                    provider: provider.provider,
                    provider_save_id: provider.save_id.clone(),
                    path: app_data.join(directory).join(provider.save_id),
                });
            }
            SaveProviderKind::LegacyVersioned => {
                let rules = all_save_path_rules();
                let game_rules = rules
                    .iter()
                    .find(|(id, _)| *id == game_id)
                    .map(|(_, rules)| *rules)
                    .unwrap_or(&[]);
                for (index, rule) in game_rules.iter().enumerate() {
                    if !version_in_range(installed_version, rule.version_from, rule.version_to) {
                        continue;
                    }
                    let path = match app_data {
                        Some(app_data) => {
                            expand_save_template_with_app_data(rule.path_template, app_data)
                        }
                        None => expand_save_template(rule.path_template),
                    };
                    if path.as_os_str().is_empty()
                        || roots
                            .iter()
                            .any(|root| path_is_same_or_nested(&path, &root.path))
                    {
                        continue;
                    }
                    roots.push(ResolvedSaveRoot {
                        index: 0,
                        provider: SaveProviderKind::LegacyVersioned,
                        provider_save_id: format!("{}-{index}", provider.save_id),
                        path,
                    });
                }
            }
        }
    }

    let mut seen = HashSet::new();
    roots.retain(|root| seen.insert(normalized_path_key(&root.path)));
    for (index, root) in roots.iter_mut().enumerate() {
        root.index = index;
    }
    roots
}

fn normalized_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

fn path_is_same_or_nested(path: &Path, root: &Path) -> bool {
    let path = normalized_path_key(path);
    let root = normalized_path_key(root);
    path == root || path.starts_with(&format!("{root}\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_DATA: &str = r"C:\Users\Test\AppData\Roaming";

    #[test]
    fn resolves_catalog_emulator_roots_without_app_id_inference() {
        let roots = resolve_save_roots_with_app_data(
            "007-first-light",
            "v1.0.0",
            Some(Path::new(APP_DATA)),
        );

        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].index, 0);
        assert_eq!(roots[0].provider, SaveProviderKind::Gse);
        assert_eq!(roots[0].provider_save_id, "3768760");
        assert!(normalized_path_key(&roots[0].path).ends_with(r"\gse saves\3768760"));
        assert_eq!(roots[1].provider, SaveProviderKind::GoldbergSteamEmu);
        assert_eq!(roots[1].index, 1);
    }

    #[test]
    fn mirage_uses_explicit_uplay_save_id() {
        let roots = resolve_save_roots_with_app_data(
            "assassins-creed-mirage",
            "1.0.0",
            Some(Path::new(APP_DATA)),
        );

        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].provider, SaveProviderKind::GoldbergUplayEmu);
        assert_eq!(roots[0].provider_save_id, "6100");
    }

    #[test]
    fn other_games_do_not_inherit_mirage_mapping() {
        let roots = resolve_save_roots_with_app_data(
            "assassins-creed-valhalla",
            "1.0.0",
            Some(Path::new(APP_DATA)),
        );

        assert!(roots.is_empty());
    }

    #[test]
    fn nested_legacy_root_is_not_backed_up_twice() {
        let roots = resolve_save_roots_with_app_data(
            "007-first-light",
            "v1.0.0",
            Some(Path::new(APP_DATA)),
        );

        assert_eq!(roots.len(), 2);
        assert!(roots
            .iter()
            .all(|root| root.provider != SaveProviderKind::LegacyVersioned));
    }
}
