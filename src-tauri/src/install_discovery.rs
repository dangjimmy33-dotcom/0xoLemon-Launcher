use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::Instant;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use uuid::Uuid;

const LIBRARY_MARKER_DIR: &str = ".0xolemon";
const LIBRARY_MARKER_FILE: &str = "library.0xo";
const LIBRARY_SCHEMA: u32 = 1;
const RECOVERY_REGISTRY_KEY: &str = r"Software\0xoLemon\LauncherLibraries";
const RECOVERY_INSTALLS_KEY: &str = r"Software\0xoLemon\LauncherLibraries\KnownInstalls";
const RECOVERY_CONFLICTS_KEY: &str = r"Software\0xoLemon\LauncherLibraries\ConflictSelections";
const COMMON_DIR: &str = "common";
const STANDARD_LIBRARY_NAME: &str = "0xoLemon store";
const LEGACY_LIBRARY_NAME: &str = "0xoLemonStore";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRecoveryIndex {
    pub schema_version: u32,
    pub library_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredInstall {
    pub game_id: String,
    pub install_path: String,
    pub version: String,
    pub launch_executable: String,
    pub applied_patch_id: Option<String>,
    pub library_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallDiscoveryConflict {
    pub game_id: String,
    pub candidate_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallDiscoveryReport {
    pub recovered: Vec<DiscoveredInstall>,
    pub conflicts: Vec<InstallDiscoveryConflict>,
    pub roots_scanned: Vec<String>,
    pub unavailable_roots: Vec<String>,
    pub invalid_candidates: usize,
    pub requires_locate_library: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GameDiscoveryView {
    pub status: String,
    pub candidate_paths: Vec<String>,
    pub library_id: Option<String>,
    pub unavailable_reason: Option<String>,
}

#[derive(Default)]
struct DiscoverySnapshot {
    completed: bool,
    generation: u64,
    automatic_jobs_generation: u64,
    requested_game_ids: BTreeSet<String>,
    games: HashMap<String, GameDiscoveryView>,
}

static DISCOVERY: OnceLock<RwLock<DiscoverySnapshot>> = OnceLock::new();
static DISCOVERY_SCAN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn discovery() -> &'static RwLock<DiscoverySnapshot> {
    DISCOVERY.get_or_init(|| RwLock::new(DiscoverySnapshot::default()))
}

fn discovery_scan_lock() -> &'static Mutex<()> {
    DISCOVERY_SCAN_LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn automatic_jobs_ready() -> bool {
    discovery()
        .read()
        .map(|snapshot| {
            snapshot.completed
                && snapshot.generation > 0
                && snapshot.automatic_jobs_generation == snapshot.generation
        })
        .unwrap_or(false)
}

pub(crate) fn has_completed_discovery() -> bool {
    discovery()
        .read()
        .map(|snapshot| snapshot.completed && snapshot.generation > 0)
        .unwrap_or(false)
}

fn requested_games_match(requested: &BTreeSet<String>, game_ids: &[String]) -> bool {
    game_ids.iter().cloned().collect::<BTreeSet<_>>() == *requested
}

pub(crate) fn completed_generation_for(game_ids: &[String]) -> Option<u64> {
    let snapshot = discovery().read().ok()?;
    (snapshot.completed
        && snapshot.generation > 0
        && requested_games_match(&snapshot.requested_game_ids, game_ids))
    .then_some(snapshot.generation)
}

pub(crate) fn activate_automatic_jobs(game_ids: &[String], generation: u64) -> Option<u64> {
    let mut snapshot = discovery().write().ok()?;
    if !snapshot.completed
        || snapshot.generation != generation
        || !requested_games_match(&snapshot.requested_game_ids, game_ids)
    {
        return None;
    }
    snapshot.automatic_jobs_generation = snapshot.generation;
    Some(snapshot.generation)
}

pub(crate) fn automatic_jobs_generation() -> Option<u64> {
    discovery().read().ok().and_then(|snapshot| {
        (snapshot.completed
            && snapshot.generation > 0
            && snapshot.automatic_jobs_generation == snapshot.generation)
            .then_some(snapshot.generation)
    })
}

pub(crate) fn game_discovery_view(game_id: &str) -> GameDiscoveryView {
    discovery()
        .read()
        .ok()
        .and_then(|snapshot| snapshot.games.get(game_id).cloned())
        .unwrap_or_default()
}

pub(crate) fn discovered_candidate_paths(game_id: &str) -> Vec<PathBuf> {
    game_discovery_view(game_id)
        .candidate_paths
        .into_iter()
        .map(PathBuf::from)
        .collect()
}

fn library_root_from_install_path(install_path: &Path) -> Option<PathBuf> {
    let common = install_path.parent()?;
    let is_common = common
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(COMMON_DIR));
    is_common.then(|| common.parent().map(Path::to_path_buf)).flatten()
}

fn volume_root(path: &Path) -> Option<PathBuf> {
    let mut components = path.components();
    let Component::Prefix(prefix) = components.next()? else {
        return None;
    };
    if !matches!(components.next(), Some(Component::RootDir)) {
        return None;
    }
    let mut root = PathBuf::from(prefix.as_os_str());
    root.push(Path::new(r"\"));
    Some(root)
}

fn path_is_on_unavailable_volume(path: &Path) -> bool {
    !path.exists() && volume_root(path).is_some_and(|root| !root.exists())
}

fn marker_path(root: &Path) -> PathBuf {
    root.join(LIBRARY_MARKER_DIR).join(LIBRARY_MARKER_FILE)
}

fn read_library_marker(root: &Path) -> Result<Option<LibraryRecoveryIndex>, String> {
    let path = marker_path(root);
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(&path)
        .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    let marker: LibraryRecoveryIndex = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid library marker {}: {error}", path.display()))?;
    if marker.schema_version != LIBRARY_SCHEMA || Uuid::parse_str(&marker.library_id).is_err() {
        return Err(format!("Unsupported library marker at {}", path.display()));
    }
    Ok(Some(marker))
}

fn write_new_library_marker(root: &Path) -> Result<LibraryRecoveryIndex, String> {
    fs::create_dir_all(root)
        .map_err(|error| format!("Could not create library {}: {error}", root.display()))?;
    let directory = root.join(LIBRARY_MARKER_DIR);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not prepare library metadata: {error}"))?;
    let marker = LibraryRecoveryIndex {
        schema_version: LIBRARY_SCHEMA,
        library_id: Uuid::new_v4().to_string(),
        created_at: Utc::now().to_rfc3339(),
    };
    let path = marker_path(root);
    let temporary = directory.join(format!(
        "{LIBRARY_MARKER_FILE}.{}.next",
        Uuid::new_v4()
    ));
    let bytes = serde_json::to_vec_pretty(&marker)
        .map_err(|error| format!("Could not serialize library marker: {error}"))?;
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("Could not create library marker: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Could not write library marker: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Could not flush library marker: {error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, &path) {
        let _ = fs::remove_file(&temporary);
        // Two startup paths may discover the same library concurrently. The
        // winner's valid marker is authoritative; a real write error remains
        // visible instead of replacing an existing identity.
        if let Some(existing) = read_library_marker(root)? {
            return Ok(existing);
        }
        return Err(format!("Could not commit library marker: {error}"));
    }
    if let Ok(directory_file) = File::open(&directory) {
        let _ = directory_file.sync_all();
    }
    Ok(marker)
}

#[cfg(windows)]
fn read_registry_roots() -> Vec<(String, PathBuf)> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(RECOVERY_REGISTRY_KEY, KEY_READ) else {
        return Vec::new();
    };
    key.enum_values()
        .filter_map(Result::ok)
        .filter_map(|(name, _)| key.get_value::<String, _>(&name).ok().map(|path| (name, PathBuf::from(path))))
        .collect()
}

#[cfg(not(windows))]
fn read_registry_roots() -> Vec<(String, PathBuf)> {
    Vec::new()
}

#[cfg(windows)]
fn read_registry_paths(key_path: &str) -> HashMap<String, PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(key_path, KEY_READ) else {
        return HashMap::new();
    };
    key.enum_values()
        .filter_map(Result::ok)
        .filter_map(|(game_id, _)| {
            key.get_value::<String, _>(&game_id)
                .ok()
                .map(|path| (game_id, PathBuf::from(path)))
        })
        .collect()
}

#[cfg(not(windows))]
fn read_registry_paths(_key_path: &str) -> HashMap<String, PathBuf> {
    HashMap::new()
}

#[cfg(windows)]
fn write_registry_path(key_path: &str, game_id: &str, install_path: &Path) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(key_path)
        .map_err(|error| format!("Could not open install recovery registry: {error}"))?;
    key.set_value(game_id, &install_path.display().to_string())
        .map_err(|error| format!("Could not save the selected install path: {error}"))
}

#[cfg(not(windows))]
fn write_registry_path(
    _key_path: &str,
    _game_id: &str,
    _install_path: &Path,
) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn delete_registry_path(key_path: &str, game_id: &str) -> Result<(), String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(key) = hkcu.open_subkey_with_flags(key_path, KEY_WRITE) else {
        return Ok(());
    };
    match key.delete_value(game_id) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not forget the selected install path: {error}")),
    }
}

#[cfg(not(windows))]
fn delete_registry_path(_key_path: &str, _game_id: &str) -> Result<(), String> {
    Ok(())
}

fn read_known_installs() -> HashMap<String, PathBuf> {
    read_registry_paths(RECOVERY_INSTALLS_KEY)
}

fn read_conflict_selections() -> HashMap<String, PathBuf> {
    read_registry_paths(RECOVERY_CONFLICTS_KEY)
}

fn write_known_install(game_id: &str, install_path: &Path) -> Result<(), String> {
    write_registry_path(RECOVERY_INSTALLS_KEY, game_id, install_path)
}

fn write_conflict_selection(game_id: &str, install_path: &Path) -> Result<(), String> {
    write_registry_path(RECOVERY_CONFLICTS_KEY, game_id, install_path)
}

pub(crate) fn forget_install_selection(game_id: &str) -> Result<(), String> {
    delete_registry_path(RECOVERY_INSTALLS_KEY, game_id)?;
    delete_registry_path(RECOVERY_CONFLICTS_KEY, game_id)
}

#[cfg(windows)]
fn write_registry_root(marker: &LibraryRecoveryIndex, root: &Path) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RECOVERY_REGISTRY_KEY)
        .map_err(|error| format!("Could not open library recovery registry: {error}"))?;
    key.set_value(&marker.library_id, &root.display().to_string())
        .map_err(|error| format!("Could not save library recovery path: {error}"))
}

#[cfg(not(windows))]
fn write_registry_root(_marker: &LibraryRecoveryIndex, _root: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn delete_registry_root(library_id: &str) -> Result<(), String> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(RECOVERY_REGISTRY_KEY)
        .map_err(|error| format!("Could not open library recovery registry: {error}"))?;
    match key.delete_value(library_id) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Could not forget library recovery path: {error}")),
    }
}

#[cfg(not(windows))]
fn delete_registry_root(_library_id: &str) -> Result<(), String> {
    Ok(())
}

pub(crate) fn remember_library_root(root: &Path) -> Result<LibraryRecoveryIndex, String> {
    if root.as_os_str().is_empty() || !root.is_absolute() {
        return Err("Library path must be an absolute path".to_string());
    }
    if root.is_file() {
        return Err(format!("Library path is a file: {}", root.display()));
    }
    let marker = match read_library_marker(root)? {
        Some(marker) => marker,
        None => write_new_library_marker(root)?,
    };
    write_registry_root(&marker, root)?;
    Ok(marker)
}

pub(crate) fn remember_install_path(install_path: &Path) -> Result<LibraryRecoveryIndex, String> {
    let root = library_root_from_install_path(install_path).ok_or_else(|| {
        format!(
            "Install path {} is not inside a library common directory",
            install_path.display()
        )
    })?;
    remember_library_root(&root)
}

pub(crate) fn remember_managed_install_path(
    install_path: &Path,
) -> Result<Option<LibraryRecoveryIndex>, String> {
    let Some(root) = library_root_from_install_path(install_path) else {
        // Existing-folder scans historically allowed a standalone game folder.
        // Such installs remain usable, but only launcher-managed `common`
        // layouts can carry a durable library identity for bounded discovery.
        return Ok(None);
    };
    remember_library_root(&root).map(Some)
}

pub(crate) fn remember_install_selection(
    game_id: &str,
    install_path: &Path,
) -> Result<(), String> {
    if game_id.trim().is_empty() || !install_path.is_absolute() {
        return Err("Install recovery requires a game ID and an absolute path".to_string());
    }
    write_known_install(game_id, install_path)
}

pub fn migrate_known_libraries(app: &AppHandle) {
    if let Ok(records) = crate::platform::install_records(app) {
        for record in records {
            let install_path = Path::new(&record.install_path);
            if install_path.is_dir() {
                if remember_managed_install_path(install_path)
                    .ok()
                    .flatten()
                    .is_some()
                {
                    let _ = remember_install_selection(&record.game_id, install_path);
                }
            }
        }
    }
    let settings = crate::platform::current_settings();
    let root = PathBuf::from(settings.default_library);
    if root.exists() {
        let _ = remember_library_root(&root);
    }
}

#[cfg(windows)]
fn local_drive_roots() -> Vec<PathBuf> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
    }
    const DRIVE_FIXED: u32 = 3;
    let mut roots = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for index in 0..26 {
        if mask & (1 << index) == 0 {
            continue;
        }
        let root = format!("{}:\\", (b'A' + index as u8) as char);
        let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();
        if unsafe { GetDriveTypeW(wide.as_ptr()) } == DRIVE_FIXED {
            roots.push(PathBuf::from(root));
        }
    }
    roots
}

#[cfg(not(windows))]
fn local_drive_roots() -> Vec<PathBuf> {
    Vec::new()
}

fn normalized_path_key(path: &Path) -> String {
    let mut value = path
        .as_os_str()
        .to_string_lossy()
        .replace('/', "\\");
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        value = format!(r"\\{unc}");
    } else if let Some(local) = value.strip_prefix(r"\\?\") {
        value = local.to_string();
    }
    value
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

fn user_visible_path(path: &Path) -> PathBuf {
    let value = path.as_os_str().to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(local) = value.strip_prefix(r"\\?\") {
        PathBuf::from(local)
    } else {
        path.to_path_buf()
    }
}

fn push_root(roots: &mut BTreeMap<String, PathBuf>, root: PathBuf) {
    if root.as_os_str().is_empty() {
        return;
    }
    roots.entry(normalized_path_key(&root)).or_insert(root);
}

fn selected_candidate<'a>(
    candidates: &'a [DiscoveredInstall],
    explicit_selection: Option<&Path>,
) -> Option<&'a DiscoveredInstall> {
    explicit_selection
        .and_then(|selected| {
            candidates.iter().find(|candidate| {
                normalized_path_key(Path::new(&candidate.install_path))
                    == normalized_path_key(selected)
            })
        })
        .or_else(|| {
            (explicit_selection.is_none() && candidates.len() == 1)
                .then(|| &candidates[0])
        })
}

fn collect_candidate_roots(app: &AppHandle) -> (Vec<PathBuf>, HashMap<String, String>) {
    let mut roots = BTreeMap::new();
    let mut unavailable_games = HashMap::new();
    if let Ok(records) = crate::platform::install_records(app) {
        for record in records {
            let path = PathBuf::from(&record.install_path);
            let root = library_root_from_install_path(&path);
            if !path.exists()
                && (root.as_ref().is_some_and(|root| !root.exists())
                    || path_is_on_unavailable_volume(&path))
            {
                unavailable_games.insert(
                    record.game_id.clone(),
                    format!("Installed library is currently unavailable: {}", path.display()),
                );
            }
            if let Some(root) = root {
                push_root(&mut roots, root);
            }
        }
    }
    if let Ok(Some(journal)) = crate::job::read_latest_journal(app) {
        if let Some(root) = library_root_from_install_path(Path::new(&journal.install_path)) {
            push_root(&mut roots, root);
        }
    }
    push_root(
        &mut roots,
        PathBuf::from(crate::platform::current_settings().default_library),
    );
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            push_root(&mut roots, directory.to_path_buf());
        }
    }
    for (_, root) in read_registry_roots() {
        push_root(&mut roots, root);
    }
    for (game_id, install_path) in read_known_installs() {
        let root = library_root_from_install_path(&install_path);
        if !install_path.exists() && root.as_ref().is_some_and(|root| !root.exists()) {
            unavailable_games.entry(game_id).or_insert_with(|| {
                format!(
                    "Selected game library is currently unavailable: {}",
                    install_path.display()
                )
            });
        } else if !install_path.exists() && !path_is_on_unavailable_volume(&install_path) {
            let _ = forget_install_selection(&game_id);
        }
        if let Some(root) = root {
            push_root(&mut roots, root);
        }
    }
    for drive in local_drive_roots() {
        push_root(&mut roots, drive.join(STANDARD_LIBRARY_NAME));
        push_root(&mut roots, drive.join(LEGACY_LIBRARY_NAME));
    }
    (roots.into_values().collect(), unavailable_games)
}

fn scan_root(
    root: &Path,
    game_ids: &[String],
) -> Result<(Option<LibraryRecoveryIndex>, Vec<DiscoveredInstall>, usize), String> {
    if !root.is_dir() {
        return Ok((None, Vec::new(), 0));
    }
    let marker = read_library_marker(root)?;
    let common = root.join(COMMON_DIR);
    if !common.is_dir() {
        return Ok((marker, Vec::new(), 0));
    }
    let canonical_common = fs::canonicalize(&common)
        .map_err(|error| format!("Could not inspect {}: {error}", common.display()))?;
    let mut installs = Vec::new();
    let mut invalid = 0;
    for entry in fs::read_dir(&common)
        .map_err(|error| format!("Could not list {}: {error}", common.display()))?
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let canonical = match fs::canonicalize(&path) {
            Ok(path) if path.parent() == Some(canonical_common.as_path()) => path,
            _ => {
                invalid += 1;
                continue;
            }
        };
        match crate::job::inspect_discoverable_install(&canonical, game_ids) {
            Ok(Some(candidate)) => installs.push(DiscoveredInstall {
                game_id: candidate.game_id,
                install_path: user_visible_path(&canonical).display().to_string(),
                version: candidate.version,
                launch_executable: candidate.launch_executable,
                applied_patch_id: candidate.applied_patch_id,
                library_id: marker.as_ref().map(|value| value.library_id.clone()),
            }),
            Ok(None) => {}
            Err(_) => invalid += 1,
        }
    }
    Ok((marker, installs, invalid))
}

pub fn discover_game_installs(
    app: &AppHandle,
    game_ids: Vec<String>,
) -> Result<InstallDiscoveryReport, String> {
    let _scan_guard = discovery_scan_lock()
        .lock()
        .map_err(|_| "Install discovery lock is unavailable".to_string())?;
    if let Ok(mut snapshot) = discovery().write() {
        snapshot.completed = false;
    }
    let started = Instant::now();
    let requested = game_ids.into_iter().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
    let requested_game_ids = requested.iter().cloned().collect::<BTreeSet<_>>();
    let existing = crate::platform::install_records(app)?
        .into_iter()
        .map(|record| (record.game_id, PathBuf::from(record.install_path)))
        .collect::<HashMap<_, _>>();
    let (roots, mut unavailable_games) = collect_candidate_roots(app);
    let registered_roots = read_registry_roots();
    let known_installs = read_known_installs();
    let conflict_selections = read_conflict_selections();
    let registered_root_keys = registered_roots
        .iter()
        .map(|(_, root)| normalized_path_key(root))
        .collect::<BTreeSet<_>>();
    let mut roots_scanned = Vec::new();
    let mut unavailable_roots = Vec::new();
    let mut invalid_candidates = 0;
    let mut grouped = BTreeMap::<String, Vec<DiscoveredInstall>>::new();

    for root in roots {
        if !root.exists() {
            if registered_root_keys.contains(&normalized_path_key(&root)) {
                unavailable_roots.push(root.display().to_string());
            }
            continue;
        }
        let (mut marker, mut installs, invalid) = match scan_root(&root, &requested) {
            Ok(result) => result,
            Err(error) => {
                invalid_candidates += 1;
                unavailable_roots.push(root.display().to_string());
                let root_key = normalized_path_key(&root);
                for (game_id, install_path) in existing
                    .iter()
                    .chain(known_installs.iter())
                {
                    if library_root_from_install_path(install_path.as_path())
                        .is_some_and(|known_root| normalized_path_key(&known_root) == root_key)
                    {
                        unavailable_games.insert(
                            game_id.clone(),
                            format!(
                                "Installed library could not be inspected at {}: {error}",
                                root.display()
                            ),
                        );
                    }
                }
                continue;
            }
        };
        invalid_candidates += invalid;
        if marker.is_none() && installs.is_empty() {
            continue;
        }
        roots_scanned.push(root.display().to_string());
        if marker.is_none() && !installs.is_empty() {
            marker = Some(remember_library_root(&root)?);
        }
        if let Some(marker) = marker {
            let _ = write_registry_root(&marker, &root);
            for install in &mut installs {
                install.library_id = Some(marker.library_id.clone());
            }
        }
        for install in installs {
            grouped.entry(install.game_id.clone()).or_default().push(install);
        }
    }

    let mut recovered = Vec::new();
    let mut conflicts = Vec::new();
    let mut views = HashMap::new();
    for game_id in requested {
        let mut candidates = grouped.remove(&game_id).unwrap_or_default();
        candidates.sort_by(|left, right| left.install_path.cmp(&right.install_path));
        candidates.dedup_by(|left, right| normalized_path_key(Path::new(&left.install_path)) == normalized_path_key(Path::new(&right.install_path)));
        if candidates.is_empty() {
            if let Some(reason) = unavailable_games.get(&game_id) {
                let selected_path = known_installs
                    .get(&game_id)
                    .map(|path| path.display().to_string())
                    .or_else(|| {
                        existing
                            .get(&game_id)
                            .map(|path| path.display().to_string())
                    });
                let library_id = selected_path.as_deref().and_then(|path| {
                    let root = library_root_from_install_path(Path::new(path))?;
                    registered_roots
                        .iter()
                        .find(|(_, registered)| {
                            normalized_path_key(registered) == normalized_path_key(&root)
                        })
                        .map(|(id, _)| id.clone())
                });
                views.insert(game_id, GameDiscoveryView {
                    status: "unavailable".to_string(),
                    candidate_paths: selected_path.into_iter().collect(),
                    library_id,
                    unavailable_reason: Some(reason.clone()),
                });
            }
            continue;
        }

        let registered_path = existing.get(&game_id);
        let explicit_selection = conflict_selections.get(&game_id);
        let selected = selected_candidate(&candidates, explicit_selection.map(PathBuf::as_path))
            .cloned();
        if let Some(install) = selected {
            let was_already_registered = registered_path.is_some_and(|registered| {
                normalized_path_key(registered) == normalized_path_key(Path::new(&install.install_path))
            });
            crate::platform::register_install(
                app,
                &install.game_id,
                Path::new(&install.install_path),
                &install.version,
                &install.launch_executable,
            )?;
            views.insert(install.game_id.clone(), GameDiscoveryView {
                status: if was_already_registered {
                    "registered".to_string()
                } else {
                    "recovered".to_string()
                },
                candidate_paths: vec![install.install_path.clone()],
                library_id: install.library_id.clone(),
                unavailable_reason: None,
            });
            if !was_already_registered {
                recovered.push(install);
            }
        } else {
            let paths = candidates.iter().map(|candidate| candidate.install_path.clone()).collect::<Vec<_>>();
            views.insert(game_id.clone(), GameDiscoveryView {
                status: "conflict".to_string(),
                candidate_paths: paths.clone(),
                ..GameDiscoveryView::default()
            });
            conflicts.push(InstallDiscoveryConflict { game_id, candidate_paths: paths });
        }
    }

    if let Ok(mut snapshot) = discovery().write() {
        snapshot.completed = true;
        snapshot.generation = snapshot.generation.saturating_add(1).max(1);
        snapshot.requested_game_ids = requested_game_ids;
        snapshot.games = views;
    }
    let requires_locate_library = existing.is_empty()
        && recovered.is_empty()
        && conflicts.is_empty()
        && unavailable_roots.is_empty();
    Ok(InstallDiscoveryReport {
        recovered,
        conflicts,
        roots_scanned,
        unavailable_roots,
        invalid_candidates,
        requires_locate_library,
        duration_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    })
}

pub fn register_library_root(path: &Path) -> Result<LibraryRecoveryIndex, String> {
    let root = if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(COMMON_DIR))
    {
        path.parent().map(Path::to_path_buf).ok_or_else(|| "Invalid common directory".to_string())?
    } else {
        path.to_path_buf()
    };
    remember_library_root(&root)
}

pub fn forget_library_root(library_id: &str) -> Result<(), String> {
    let root = read_registry_roots()
        .into_iter()
        .find(|(id, _)| id == library_id)
        .map(|(_, root)| root);
    if let Some(root) = root {
        let selection_ids = read_known_installs()
            .into_iter()
            .filter_map(|(game_id, install_path)| {
                library_root_from_install_path(&install_path)
                    .is_some_and(|selected_root| {
                        normalized_path_key(&selected_root) == normalized_path_key(&root)
                    })
                    .then_some(game_id)
            })
            .collect::<Vec<_>>();
        for game_id in selection_ids {
            forget_install_selection(&game_id)?;
        }
        let marker = marker_path(&root);
        let marker_matches = read_library_marker(&root)
            .ok()
            .flatten()
            .is_some_and(|marker| marker.library_id == library_id);
        if marker_matches && marker.is_file() {
            fs::remove_file(&marker)
                .map_err(|error| format!("Could not remove library marker: {error}"))?;
        }
    }
    delete_registry_root(library_id)?;
    Ok(())
}

pub fn resolve_install_conflict(
    app: &AppHandle,
    game_id: &str,
    install_path: &Path,
) -> Result<DiscoveredInstall, String> {
    let canonical = fs::canonicalize(install_path).map_err(|error| {
        format!(
            "Could not resolve the selected install path {}: {error}",
            install_path.display()
        )
    })?;
    let visible_install_path = user_visible_path(&canonical);
    let selected_key = normalized_path_key(&canonical);
    let is_known_conflict = discovery()
        .read()
        .ok()
        .and_then(|snapshot| snapshot.games.get(game_id).cloned())
        .is_some_and(|view| {
            view.status == "conflict"
                && view
                    .candidate_paths
                    .iter()
                    .any(|candidate| normalized_path_key(Path::new(candidate)) == selected_key)
        });
    if !is_known_conflict {
        return Err("The selected folder is not a current discovery candidate".to_string());
    }
    if library_root_from_install_path(&visible_install_path).is_none() {
        return Err("The selected install is not a direct child of a library common directory".to_string());
    }
    let marker = crate::job::inspect_discoverable_install(&canonical, &[game_id.to_string()])?
        .ok_or_else(|| "The selected folder is not a valid install for this game".to_string())?;
    let library = remember_install_path(&visible_install_path)?;
    crate::platform::register_install(
        app,
        &marker.game_id,
        &visible_install_path,
        &marker.version,
        &marker.launch_executable,
    )?;
    write_known_install(&marker.game_id, &visible_install_path)?;
    write_conflict_selection(&marker.game_id, &visible_install_path)?;
    let install = DiscoveredInstall {
        game_id: marker.game_id.clone(),
        install_path: visible_install_path.display().to_string(),
        version: marker.version,
        launch_executable: marker.launch_executable,
        applied_patch_id: marker.applied_patch_id,
        library_id: Some(library.library_id.clone()),
    };
    if let Ok(mut snapshot) = discovery().write() {
        snapshot.games.insert(marker.game_id, GameDiscoveryView {
            status: "recovered".to_string(),
            candidate_paths: vec![install.install_path.clone()],
            library_id: Some(library.library_id),
            unavailable_reason: None,
        });
    }
    Ok(install)
}

#[cfg(test)]
mod tests {
    use super::{
        library_root_from_install_path, normalized_path_key, selected_candidate, user_visible_path,
        volume_root, DiscoveredInstall,
    };
    use std::path::Path;

    fn candidate(path: &str) -> DiscoveredInstall {
        DiscoveredInstall {
            game_id: "game".to_string(),
            install_path: path.to_string(),
            version: "1.0".to_string(),
            launch_executable: "game.exe".to_string(),
            applied_patch_id: None,
            library_id: None,
        }
    }

    #[test]
    fn derives_library_root_only_from_common_layout() {
        assert_eq!(
            library_root_from_install_path(Path::new(r"E:\Games\common\Title")),
            Some(Path::new(r"E:\Games").to_path_buf())
        );
        assert!(library_root_from_install_path(Path::new(r"E:\Games\Title")).is_none());
    }

    #[test]
    fn derives_drive_root_without_scanning_parent_directories() {
        assert_eq!(
            volume_root(Path::new(r"E:\Games\Standalone\Game")),
            Some(Path::new(r"E:\").to_path_buf())
        );
    }

    #[test]
    fn path_identity_is_case_and_separator_insensitive_on_windows() {
        assert_eq!(
            normalized_path_key(Path::new(r"E:\0xoLemon store\common\Game\")),
            normalized_path_key(Path::new(r"e:/0XOLEMON STORE/common/Game")),
        );
        assert_eq!(
            normalized_path_key(Path::new(r"\\?\E:\0xoLemon store\common\Game")),
            normalized_path_key(Path::new(r"E:\0xoLemon store\common\Game")),
        );
        assert_eq!(
            normalized_path_key(Path::new(r"\\?\UNC\server\share\common\Game")),
            normalized_path_key(Path::new(r"\\server\share\common\Game")),
        );
    }

    #[test]
    fn user_visible_paths_remove_windows_verbatim_prefixes() {
        assert_eq!(
            user_visible_path(Path::new(r"\\?\E:\Library\common\Game")),
            Path::new(r"E:\Library\common\Game")
        );
        assert_eq!(
            user_visible_path(Path::new(r"\\?\UNC\server\share\common\Game")),
            Path::new(r"\\server\share\common\Game")
        );
    }

    #[test]
    fn a_single_candidate_is_selected_without_user_input() {
        let candidates = vec![candidate(r"E:\Library\common\Game")];
        assert_eq!(
            selected_candidate(&candidates, None).map(|value| value.install_path.as_str()),
            Some(r"E:\Library\common\Game")
        );
    }

    #[test]
    fn duplicate_installs_require_an_explicit_selection() {
        let candidates = vec![
            candidate(r"D:\Library\common\Game"),
            candidate(r"E:\Library\common\Game"),
        ];
        assert!(selected_candidate(&candidates, None).is_none());
        assert_eq!(
            selected_candidate(
                &candidates,
                Some(Path::new(r"\\?\e:\library\common\game")),
            )
            .map(|value| value.install_path.as_str()),
            Some(r"E:\Library\common\Game")
        );
    }

    #[test]
    fn a_stale_selection_never_falls_back_to_another_copy() {
        let candidates = vec![candidate(r"E:\Library\common\Game")];
        assert!(selected_candidate(
            &candidates,
            Some(Path::new(r"D:\OfflineLibrary\common\Game")),
        )
        .is_none());
    }
}
