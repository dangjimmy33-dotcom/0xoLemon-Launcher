/// Version-aware save game path mapping for all games.
///
/// Each entry maps a game_id to a list of `VersionedSavePath`.
/// When a game exits, the launcher resolves the correct save path for the
/// installed version and copies those files to a local snapshot directory.
use std::path::PathBuf;

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
