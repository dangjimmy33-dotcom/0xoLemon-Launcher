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

fn patch_transfer_bytes(manifest: &VersionManifest) -> u64 {
    manifest
        .files
        .iter()
        .flat_map(|file| file.chunks.iter())
        .map(|chunk| chunk.compressed_size)
        .sum()
}

pub fn check_patch_available(game_id: &str, version: &str) -> Result<Option<String>, String> {
    let source = DepotSource::for_game(game_id);
    load_patch_manifest(&source, version)
        .map(|manifest| manifest.map(|manifest| manifest.created_at))
        .map_err(|error| error.to_string())
}

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