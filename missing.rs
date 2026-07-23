#[derive(Debug, Clone)]
struct ReconciledInstall {
    game_id: String,
    install_path: PathBuf,
    marker: InstallMarker,
}

const AUTO_PATCH_POLL_INTERVAL: Duration = Duration::from_secs(60);
const AUTO_PATCH_RETRY_BACKOFF: Duration = Duration::from_secs(5 * 60);

    let patch_app = app.clone();
    let patch_control = control.clone();
    thread::spawn(move || {
        let mut last_attempts = HashMap::<String, (String, Instant)>::new();
        loop {
            if let Err(error) = auto_patch_tick(&patch_app, &patch_control, &mut last_attempts) {
                let _ = patch_app.emit(
                    "launcher://auto-update",
                    AutoUpdateEvent {
                        state: "error".to_string(),
                        message: error,
                        game_id: None,
                    },
                );
            }
            thread::sleep(AUTO_PATCH_POLL_INTERVAL);
        }
    });

fn automatic_job_can_start(app: &AppHandle, control: &Arc<JobControl>) -> Result<bool, String> {
    if control.is_running() {
        return Ok(false);
    }
    if crate::platform::get_runtime_states(app)?
        .iter()
        .any(|runtime| runtime.running)
    {
        return Ok(false);
    }
    if read_latest_journal(app)
        .map_err(|error| error.to_string())?
        .is_some_and(|journal| {
            matches!(
                journal.status,
                JobStatus::Planned
                    | JobStatus::Running
                    | JobStatus::Paused
                    | JobStatus::Downloading
                    | JobStatus::Assembling
                    | JobStatus::Verified
            )
        })
    {
        return Ok(false);
    }
    Ok(true)
}

fn patch_attempt_is_throttled(
    last_attempts: &HashMap<String, (String, Instant)>,
    game_id: &str,
    patch_id: &str,
) -> bool {
    last_attempts.get(game_id).is_some_and(|(last_patch_id, attempted_at)| {
        last_patch_id == patch_id && attempted_at.elapsed() < AUTO_PATCH_RETRY_BACKOFF
    })
}

fn auto_patch_tick(
    app: &AppHandle,
    control: &Arc<JobControl>,
    last_attempts: &mut HashMap<String, (String, Instant)>,
) -> Result<(), String> {
    let installs = reconciled_installs(app)?;
    if !automatic_job_can_start(app, control)? {
        return Ok(());
    }

    for install in installs {
        let Some(version) = usable_installed_version(&install.marker.version) else {
            continue;
        };
        let source = DepotSource::for_game(&install.game_id);
        let patch_manifest = match load_patch_manifest(&source, &version) {
            Ok(Some(manifest)) => manifest,
            Ok(None) => continue,
            Err(error) => {
                if !patch_attempt_is_throttled(
                    last_attempts,
                    &install.game_id,
                    "manifest-unavailable",
                ) {
                    last_attempts.insert(
                        install.game_id.clone(),
                        ("manifest-unavailable".to_string(), Instant::now()),
                    );
                    let _ = app.emit(
                        "launcher://auto-update",
                        AutoUpdateEvent {
                            state: "error".to_string(),
                            message: format!(
                                "Could not check hotfixes for {}: {}",
                                install.game_id, error
                            ),
                            game_id: Some(install.game_id.clone()),
                        },
                    );
                }
                continue;
            }
        };
        let patch_id = patch_manifest.created_at.trim();
        if patch_id.is_empty()
            || install.marker.applied_patch_id.as_deref() == Some(patch_id)
            || patch_attempt_is_throttled(last_attempts, &install.game_id, patch_id)
        {
            continue;
        }

        last_attempts.insert(
            install.game_id.clone(),
            (patch_id.to_string(), Instant::now()),
        );
        let _ = app.emit(
            "launcher://auto-update",
            AutoUpdateEvent {
                state: "patching".to_string(),
                message: format!("Applying hotfix for {} ({})", install.game_id, version),
                game_id: Some(install.game_id.clone()),
            },
        );
        spawn_patch_job(
            app.clone(),
            control.clone(),
            install.install_path.display().to_string(),
            Some(version),
            Some(install.game_id),
        )
        .map_err(|error| error.to_string())?;
        break;
    }
    Ok(())
}

    let installs = reconciled_installs(app)?;
    if !automatic_job_can_start(app, control)? {
        let Some(installed_version) = usable_installed_version(&install.marker.version) else {
            continue;
        };
        if latest == installed_version {
            install.install_path.display().to_string(),
fn load_patch_manifest(
    source: &DepotSource,
    version: &str,
) -> Result<Option<VersionManifest>, JobError> {
    let version = usable_installed_version(version).ok_or_else(|| {
        JobError::Depot("cannot check a patch for an unknown installed version".to_string())
    })?;
    let patch_path = format!("patches/{version}/manifest.json");
    match source.load_json::<VersionManifest>(&patch_path) {
        Ok(manifest) => Ok(Some(manifest)),
        Err(JobError::NotFound(_)) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn check_patch_available(game_id: &str, version: &str) -> Result<Option<String>, String> {
    let source = DepotSource::for_game(game_id);
    load_patch_manifest(&source, version)
        .map(|manifest| manifest.map(|manifest| manifest.created_at))
        .map_err(|error| error.to_string())
}

fn resolve_patch_target_version(
    source: &DepotSource,
    install_root: &Path,
    requested_version: Option<String>,
) -> Result<String, JobError> {
    if !install_root.is_dir() {
        return Err(JobError::Depot(format!(
            "{} is not installed at '{}'",
            source.game_dir_name,
            install_root.display()
        )));
    }

    let marker = read_install_marker(install_root)?.ok_or_else(|| {
        JobError::Depot(format!(
            "{} is missing .0xolemon/state.0xo",
            source.game_dir_name
        ))
    })?;
    if !install_marker_matches_source(&marker, source) {
        return Err(JobError::Depot(format!(
            "install metadata belongs to '{}', not '{}'",
            marker.game_id, source.game_id
        )));
    }

    let installed_version = usable_installed_version(&marker.version).ok_or_else(|| {
        JobError::Depot("cannot apply a patch to an install with an unknown version".to_string())
    })?;
    if let Some(requested_version) = requested_version {
        let requested_version = usable_installed_version(&requested_version).ok_or_else(|| {
            JobError::Depot("cannot apply a patch for an unknown target version".to_string())
        })?;
        if requested_version != installed_version {
            return Err(JobError::Depot(format!(
                "patch target '{}' does not match installed version '{}'",
                requested_version, installed_version
            )));
        }
    }

    Ok(installed_version)
    let install_root = PathBuf::from(&install_path);
    let target_version = resolve_patch_target_version(&source, &install_root, target_version)?;
    resolve_patch_target_version(&source, &install_root, Some(journal.to_version.clone()))?;
    let patch_manifest: VersionManifest = match load_patch_manifest(source, version) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
fn patch_target_version_from_marker(
    source: &DepotSource,
    marker: &InstallMarker,
    requested_version: Option<String>,
) -> Result<String, JobError> {

fn resolve_patch_target_version(
    source: &DepotSource,
    install_root: &Path,
    requested_version: Option<String>,
) -> Result<String, JobError> {
    if !install_root.is_dir() {
        return Err(JobError::Depot(format!(
            "{} is not installed at '{}'",
            source.game_dir_name,
            install_root.display()
        )));
    }

    let marker = read_install_marker(install_root)?.ok_or_else(|| {
        JobError::Depot(format!(
            "{} is missing .0xolemon/state.0xo",
            source.game_dir_name
        ))
    })?;
    patch_target_version_from_marker(source, &marker, requested_version)
}
    #[test]
    fn patch_retry_backoff_is_scoped_to_the_patch_identity() {
        let mut attempts = HashMap::new();
        attempts.insert(
            "geometry-dash".to_string(),
            ("patch-1".to_string(), Instant::now()),
        );

        assert!(patch_attempt_is_throttled(&attempts, "geometry-dash", "patch-1"));
        assert!(!patch_attempt_is_throttled(&attempts, "geometry-dash", "patch-2"));
        assert!(!patch_attempt_is_throttled(&attempts, "another-game", "patch-1"));
    }

    #[test]
    fn patch_job_uses_the_committed_marker_version_without_catalog_lookup() {
        let source = DepotSource::for_game("geometry-dash");
        let marker = InstallMarker {
            game_id: "geometry-dash".to_string(),
            version: "2.2081-hotfixable".to_string(),
            installed_at: String::new(),
            launch_executable: None,
            applied_patch_id: None,
        };

        assert_eq!(
            patch_target_version_from_marker(&source, &marker, None).unwrap(),
            "2.2081-hotfixable"
        );
        assert_eq!(
            patch_target_version_from_marker(
                &source,
                &marker,
                Some("2.2081-hotfixable".to_string()),
            )
            .unwrap(),
            "2.2081-hotfixable"
        );
    }

    #[test]
    fn patch_job_rejects_a_version_that_does_not_match_the_marker() {
        let source = DepotSource::for_game("geometry-dash");
        let marker = InstallMarker {
            game_id: "geometry-dash".to_string(),
            version: "2.2081".to_string(),
            installed_at: String::new(),
            launch_executable: None,
            applied_patch_id: None,
        };

        assert!(patch_target_version_from_marker(
            &source,
            &marker,
            Some("2.2082".to_string()),
        )
        .is_err());
    }

    #[test]
    fn active_patch_journals_are_restored_with_other_real_jobs() {
        let journal = default_journal(
            "geometry-dash",
            "patch",
            r"E:\0xoLemon store\common\Geometry Dash".to_string(),
            "2.2081",
            "2.2081",
            0,
        );
        assert!(is_active_real_journal(&journal));
    }


fn applied_patch_manifest_path(install_root: &Path) -> PathBuf {
    install_root
        .join(INSTALL_MARKER_DIR)
        .join(APPLIED_PATCH_MANIFEST_FILE)
}

fn write_applied_patch_manifest(
    install_root: &Path,
    manifest: &VersionManifest,
) -> Result<(), JobError> {
    let path = applied_patch_manifest_path(install_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    write_state_file(&path, manifest)
}

fn read_applied_patch_manifest(
    install_root: &Path,
) -> Result<Option<VersionManifest>, JobError> {
    let path = applied_patch_manifest_path(install_root);
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(read_state_file(&path)?))
}

fn clear_applied_patch_manifest(install_root: &Path) {
    let path = applied_patch_manifest_path(install_root);
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn manifest_file_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn overlay_patch_manifest(
    mut base_manifest: VersionManifest,
    patch_manifest: &VersionManifest,
) -> Result<VersionManifest, JobError> {
    let mut patch_by_path = HashMap::<String, &FileEntry>::new();
    for file in &patch_manifest.files {
        let key = manifest_file_key(&file.path);
        if key.is_empty() || patch_by_path.insert(key.clone(), file).is_some() {
            return Err(JobError::Depot(format!(
                "patch manifest contains a duplicate or empty file path: {}",
                file.path
            )));
        }
    }

    let mut base_paths = HashSet::new();
    let mut merged_files = Vec::with_capacity(base_manifest.files.len() + patch_manifest.files.len());
    for file in base_manifest.files {
        let key = manifest_file_key(&file.path);
        base_paths.insert(key.clone());
        merged_files.push(
            patch_by_path
                .get(&key)
                .copied()
                .cloned()
                .unwrap_or(file),
        );
    }
    for file in &patch_manifest.files {
        if !base_paths.contains(&manifest_file_key(&file.path)) {
            merged_files.push(file.clone());
        }
    }

    base_manifest.total_size = merged_files.iter().map(|file| file.size).sum();
    base_manifest.files = merged_files;
    // The combined local manifest is intentionally not represented as a
    // depot-signed manifest. Each replacement still has its own SHA-256.
    base_manifest.signature = None;
    Ok(base_manifest)
}

fn applied_patch_manifest_for_marker(
    source: &DepotSource,
    install_root: &Path,
    marker: &InstallMarker,
) -> Result<Option<VersionManifest>, JobError> {
    let Some(applied_patch_id) = marker.applied_patch_id.as_deref() else {
        return Ok(None);
    };
    let version = usable_installed_version(&marker.version).ok_or_else(|| {
        JobError::Depot("cannot verify a patch for an unknown installed version".to_string())
    })?;
    let manifest = match read_applied_patch_manifest(install_root)? {
        Some(manifest) => {
            validate_patch_manifest(source, &version, &manifest)?;
            manifest
        }
        None => load_patch_manifest(source, &version)?.ok_or_else(|| {
            JobError::Depot(format!(
                "the applied patch '{}' no longer has a manifest to verify",
                applied_patch_id
            ))
        })?,
    };
    if manifest.created_at != applied_patch_id {
        return Err(JobError::Depot(format!(
            "applied patch marker '{}' does not match manifest '{}'",
            applied_patch_id, manifest.created_at
        )));
    }
    Ok(Some(manifest))
}

fn installed_manifest_for_version(
    source: &DepotSource,
    install_root: &Path,
    marker: Option<&InstallMarker>,
    version: &str,
) -> Result<VersionManifest, JobError> {
    let marker_matches_version = marker.is_some_and(|marker| {
        usable_installed_version(&marker.version) == usable_installed_version(version)
    });
    let base_manifest = if marker_matches_version {
        match read_installed_manifest(install_root)? {
            Some(manifest) => manifest,
            None => load_manifest_for_version(source, version)?,
        }
    } else {
        load_manifest_for_version(source, version)?
    };

    if marker_matches_version {
        if let Some(marker) = marker {
            if let Some(patch_manifest) = applied_patch_manifest_for_marker(source, install_root, marker)? {
                return overlay_patch_manifest(base_manifest, &patch_manifest);
            }
        }
    }
    Ok(base_manifest)
}
    clear_applied_patch_manifest(install_root);
    clear_applied_patch_manifest(install_root);
            validate_patch_manifest(source, &version, &manifest)?;

fn validate_patch_manifest(
    source: &DepotSource,
    version: &str,
    manifest: &VersionManifest,
) -> Result<(), JobError> {
    let manifest_game_matches = sanitize_game_id(&manifest.game_id) == source.game_id
        || compact_game_id(&manifest.game_id) == compact_game_id(&source.game_dir_name);
    if !manifest_game_matches {
        return Err(JobError::Depot(format!(
            "patch manifest belongs to '{}', not '{}'",
            manifest.game_id, source.game_id
        )));
    }
    let manifest_version = usable_installed_version(&manifest.version)
        .ok_or_else(|| JobError::Depot("patch manifest is missing a usable version".to_string()))?;
    if manifest_version != version {
        return Err(JobError::Depot(format!(
            "patch manifest version '{}' does not match '{}'",
            manifest_version, version
        )));
    }
    if manifest.created_at.trim().is_empty() {
        return Err(JobError::Depot(
            "patch manifest is missing createdAt".to_string(),
        ));
    }
    Ok(())
}

fn default_patch_journal(
    source: &DepotSource,
    install_path: String,
    target_version: &str,
) -> JobJournal {
    let mut journal = default_journal(
        &source.game_id,
        "patch",
        install_path,
        "patching",
        target_version,
        0,
    );
    journal.steps = vec![
        step("Check patch", "Confirm the installed version before patching"),
        step("Download patch", "Download version-specific patch files"),
        step("Verify patch", "Validate downloaded patch files"),
        step("Apply patch", "Replace patched game files after validation"),
        step("Record patch", "Persist the applied patch metadata"),
        step("Patch complete", "Waiting for patch completion"),
    ];
    if let Some(log) = journal.logs.first_mut() {
        log.message = "Ready to download and apply a resumable patch".to_string();
    }
    journal
}
    let journal = default_patch_journal(&source, install_path, &target_version);
    set_step_running(app, &mut journal, 0, JobStatus::Running, "Check patch")?;
    resolve_patch_target_version(&source, &install_root, Some(journal.to_version.clone()))?;
    complete_step(app, &mut journal, 0)?;
fn patch_transfer_bytes(manifest: &VersionManifest) -> u64 {
    manifest
        .files
        .iter()
        .flat_map(|file| file.chunks.iter())
        .map(|chunk| chunk.compressed_size)
        .sum()
}

fn try_apply_patch_fix(
    app: &AppHandle,
    journal: &mut JobJournal,
    source: &DepotSource,
    install_root: &Path,
    version: &str,
    control: &Arc<JobControl>,
    manifest_errors_are_fatal: bool,
) -> Result<(), JobError> {
    let standalone_patch = journal.kind == "patch";
    let patch_step_index = if standalone_patch { 1 } else { 5 };
    let patch_manifest: VersionManifest = match load_patch_manifest(source, version) {
        Ok(Some(manifest)) => manifest,
        Ok(None) => {
            append_log(journal, "info", &format!("[Patch fix] No patch for {} - skipping", version));
            if standalone_patch {
                for step in journal.steps.iter_mut().skip(patch_step_index) {
                    step.status = StepStatus::Completed;
                    step.detail = "No patch required for this version".to_string();
                    step.progress = 1.0;
                }
            } else if let Some(step) = journal.steps.get_mut(patch_step_index) {
                step.status = StepStatus::Completed;
                step.detail = "No patch required for this version".to_string();
                step.progress = 1.0;
            }
            journal.overall_progress = 1.0;
            journal.phase = if standalone_patch {
                "No patch required".to_string()
            } else {
                "Committed".to_string()
            };
            journal.status = JobStatus::Committed;
            persist_and_emit(app, journal)?;
            return Ok(());
        }
        Err(err) => {
            if manifest_errors_are_fatal {
                append_log(journal, "error", &format!("[Patch fix] Could not load patch manifest: {err}"));
                if let Some(step) = journal.steps.get_mut(patch_step_index) {
                    step.status = StepStatus::Failed;
                    step.detail = "Patch manifest unavailable".to_string();
                }
                journal.phase = "Failed".to_string();
                journal.status = JobStatus::Failed;
                persist_and_emit(app, journal)?;
                return Err(err);
            }

            append_log(journal, "warning", &format!("[Patch fix] Could not load patch manifest: {err} - skipping"));
            if let Some(step) = journal.steps.get_mut(patch_step_index) {
                step.status = StepStatus::Completed;
                step.detail = "Skipped (patch manifest unavailable)".to_string();
                step.progress = 1.0;
            }
            journal.overall_progress = 1.0;
            journal.phase = "Committed".to_string();
            journal.status = JobStatus::Committed;
            persist_and_emit(app, journal)?;
            return Ok(());
        }
    };

    let file_count = patch_manifest.files.len();
    append_log(
        journal,
        "info",
        &format!("[Patch fix] Found patch for {} ({} file(s))", version, file_count),
    );

    let patch_chunks = patch_manifest
        .files
        .iter()
        .flat_map(|file| file.chunks.iter().cloned())
        .collect::<Vec<_>>();
    if standalone_patch {
        journal.bytes_total = patch_transfer_bytes(&patch_manifest);
        journal.bytes_done = 0;
        configure_download_metrics(journal, &patch_chunks, true);
        journal.metrics.pipeline = "patch-direct-v1".to_string();
        set_step_running(app, journal, patch_step_index, JobStatus::Downloading, "Download patch")?;
        if let Some(step) = journal.steps.get_mut(patch_step_index) {
            step.detail = format!("Downloading {} patch file(s)...", file_count);
        }
        persist_and_emit(app, journal)?;
    } else {
        journal.status = JobStatus::Running;
        journal.phase = "Patch fix".to_string();
        if let Some(step) = journal.steps.get_mut(patch_step_index) {
            step.status = StepStatus::Running;
            step.progress = 0.0;
            step.detail = format!("Applying {} fix file(s)...", file_count);
        }
        journal.overall_progress = overall_progress(patch_step_index, 0.0);
        persist_and_emit(app, journal)?;
    }

    let patch_stage = install_root.join(INSTALL_MARKER_DIR).join("patch_stage");
    fs::create_dir_all(&patch_stage)?;
    let (fallback_progress_tx, _fallback_progress_rx) =
        mpsc::channel::<Result<DownloadProgress, String>>();
    let mut downloaded = 0_u64;
    let mut in_flight = HashMap::<String, u64>::new();

    for (idx, file) in patch_manifest.files.iter().enumerate() {
        if standalone_patch {
            wait_for_control(app, control, journal, patch_step_index)?;
        } else if control.is_canceled() {
            return Err(JobError::Canceled);
        }
        let target_path = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe patch path: {}", file.path)))?;
        append_log(
            journal,
            "info",
            &format!("[Patch fix] ({}/{}) {}", idx + 1, file_count, file.path),
        );
        let mut assembled = Vec::with_capacity(file.size as usize);
        for chunk in &file.chunks {
            let pack_relative = format!("patches/{}/packs/{}.bin", version, chunk.pack_id);
            let task_id = format!("patch-{}-{}", version, chunk.hash);
            let partial_path = patch_stage.join(format!("{}.partial", chunk.hash));
            let transport = if standalone_patch {
                fetch_patch_pack_span_with_journal_progress(
                    app,
                    journal,
                    source,
                    chunk,
                    pack_relative,
                    task_id,
                    partial_path,
                    control,
                    patch_step_index,
                    &mut downloaded,
                    &mut in_flight,
                )?
            } else {
                source.fetch_pack_span_with_progress(
                    &chunk.pack_id,
                    chunk.pack_offset,
                    chunk.pack_offset + chunk.compressed_size,
                    &pack_relative,
                    &task_id,
                    &partial_path,
                    control,
                    &fallback_progress_tx,
                )?
            };
            verify_compressed_chunk_bytes(chunk, &transport)?;
            let compressed = decode_transport_chunk(chunk, &transport)?;
            let plain = decode_chunk_payload(chunk, &compressed)?;
            verify_chunk_bytes(chunk, &plain)?;
            assembled.extend_from_slice(&plain);
        }
        if !sha256_bytes(&assembled).eq_ignore_ascii_case(&file.sha256) {
            return Err(JobError::Depot(format!(
                "patch file hash mismatch: {}",
                file.path
            )));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let target_path = long_path(&target_path);
        let tmp_path = target_path.with_extension("patch_tmp");
        fs::write(&tmp_path, &assembled)?;
        fs::rename(&tmp_path, &target_path)?;
        if !standalone_patch {
            let step_progress = progress_fraction(idx + 1, file_count);
            if let Some(step) = journal.steps.get_mut(patch_step_index) {
                step.progress = step_progress;
            }
            journal.overall_progress = overall_progress(patch_step_index, step_progress);
            persist_and_emit(app, journal)?;
        }
    }
    for file in &patch_manifest.files {
        for chunk in &file.chunks {
            let partial_path = patch_stage.join(format!("{}.partial", chunk.hash));
            let _ = fs::remove_file(&partial_path);
        }
    }
    let _ = fs::remove_dir(&patch_stage);

    if standalone_patch {
        complete_step(app, journal, 1)?;
        set_step_running(app, journal, 2, JobStatus::Running, "Verify patch")?;
        if let Some(step) = journal.steps.get_mut(2) {
            step.detail = format!("Validated {} patch file(s)", file_count);
        }
        complete_step(app, journal, 2)?;
        set_step_running(app, journal, 3, JobStatus::Running, "Apply patch")?;
        if let Some(step) = journal.steps.get_mut(3) {
            step.detail = format!("Applied {} patch file(s)", file_count);
        }
        complete_step(app, journal, 3)?;
        set_step_running(app, journal, 4, JobStatus::Running, "Record patch")?;
    }

    write_applied_patch_manifest(install_root, &patch_manifest)?;
    let mut marker = read_install_marker(install_root)?.ok_or_else(|| {
        JobError::Depot(format!(
            "{} is missing .0xolemon/state.0xo after patching",
            source.game_dir_name
        ))
    })?;
    patch_target_version_from_marker(source, &marker, Some(version.to_string()))?;
    marker.applied_patch_id = Some(patch_manifest.created_at.clone());
    write_install_marker_file(install_root, &marker)?;

    append_log(journal, "info", &format!("[Patch fix] Applied {} file(s)", file_count));
    if standalone_patch {
        complete_step(app, journal, 4)?;
        set_step_running(app, journal, 5, JobStatus::Running, "Finalize patch")?;
        if let Some(step) = journal.steps.get_mut(5) {
            step.detail = format!("Patch applied to {}", version);
        }
        complete_step(app, journal, 5)?;
        journal.bytes_done = journal.bytes_total;
    } else if let Some(step) = journal.steps.get_mut(patch_step_index) {
        step.status = StepStatus::Completed;
        step.progress = 1.0;
        step.detail = format!("Applied {} fix file(s)", file_count);
    }
    journal.overall_progress = 1.0;
    journal.phase = if standalone_patch {
        "Patch applied".to_string()
    } else {
        "Committed".to_string()
    };
    journal.status = JobStatus::Committed;
    persist_and_emit(app, journal)?;
    Ok(())
}

fn record_patch_download_progress(
    app: &AppHandle,
    journal: &mut JobJournal,
    control: &JobControl,
    step_index: usize,
    downloaded: &mut u64,
    in_flight: &mut HashMap<String, u64>,
    progress: DownloadProgress,
) -> Result<(), JobError> {
    wait_for_control(app, control, journal, step_index)?;
    if progress.clear_in_flight {
        in_flight.remove(&progress.task_id);
    } else {
        in_flight.insert(progress.task_id.clone(), progress.in_flight_bytes);
    }
    *downloaded = downloaded.saturating_add(progress.committed_bytes);
    let in_flight_bytes = in_flight.values().copied().sum::<u64>();
    observe_download_progress(journal, &progress, in_flight_bytes);
    journal.bytes_done = downloaded
        .saturating_add(in_flight_bytes)
        .min(journal.bytes_total);
    journal.steps[step_index].progress = byte_progress(journal.bytes_done, journal.bytes_total);
    journal.steps[step_index].retry_count = journal.steps[step_index]
        .retry_count
        .max(progress.retry_count);
    journal.steps[step_index].detail = format!(
        "Downloading patch files ({} / {})",
        human_bytes(journal.bytes_done),
        human_bytes(journal.bytes_total)
    );
    journal.overall_progress = overall_progress(step_index, journal.steps[step_index].progress);
    touch(journal);
    persist_and_emit(app, journal)
}

#[allow(clippy::too_many_arguments)]
fn fetch_patch_pack_span_with_journal_progress(
    app: &AppHandle,
    journal: &mut JobJournal,
    source: &DepotSource,
    chunk: &ChunkRef,
    pack_relative: String,
    task_id: String,
    partial_path: PathBuf,
    control: &Arc<JobControl>,
    step_index: usize,
    downloaded: &mut u64,
    in_flight: &mut HashMap<String, u64>,
) -> Result<Vec<u8>, JobError> {
    let (progress_tx, progress_rx) = mpsc::channel::<Result<DownloadProgress, String>>();
    let source = source.clone();
    let pack_id = chunk.pack_id.clone();
    let start = chunk.pack_offset;
    let end_exclusive = chunk.pack_offset + chunk.compressed_size;
    let expected_size = chunk.compressed_size;
    let worker_task_id = task_id.clone();
    let worker_control = Arc::clone(control);
    let worker = thread::spawn(move || {
        source.fetch_pack_span_with_progress(
            &pack_id,
            start,
            end_exclusive,
            &pack_relative,
            &worker_task_id,
            &partial_path,
            &worker_control,
            &progress_tx,
        )
    });

    let mut interruption = None;
    loop {
        match progress_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(Ok(progress)) => record_patch_download_progress(
                app,
                journal,
                control,
                step_index,
                downloaded,
                in_flight,
                progress,
            )?,
            Ok(Err(message)) => {
                control.cancel();
                interruption = Some(JobError::Depot(message));
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Err(error) = wait_for_control(app, control, journal, step_index) {
                    interruption = Some(error);
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let worker_result = worker
        .join()
        .map_err(|_| JobError::Depot("patch download worker panicked".to_string()))?;
    if let Some(error) = interruption {
        let _ = worker_result;
        return Err(error);
    }
    let transport = worker_result?;
    record_patch_download_progress(
        app,
        journal,
        control,
        step_index,
        downloaded,
        in_flight,
        DownloadProgress {
            task_id,
            committed_bytes: expected_size,
            in_flight_bytes: 0,
            clear_in_flight: true,
            retry_count: 0,
            rate_bytes_per_second: 0,
            retry_wait_ms: 0,
            rate_limit_wait_ms: 0,
        },
    )?;
    Ok(transport)
}
