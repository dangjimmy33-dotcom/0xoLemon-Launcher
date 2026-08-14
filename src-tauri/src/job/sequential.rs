use super::*;

const SEQUENTIAL_STAGE_SCHEMA: u32 = 3;
const SESSION_DIRECTORY: &str = "s";
const STAGE_DIRECTORY: &str = "f";
const CACHE_DIRECTORY: &str = "c";
const STATE_FILE: &str = "state.json";
const TRANSACTION_FILE: &str = "txn.json";
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(2);
const STATE_PERSIST_INTERVAL: Duration = Duration::from_secs(2);
const STATE_PERSIST_BYTES: u64 = 64 * 1024 * 1024;
const STAGE_SYNC_WORKERS: usize = 8;
const FILE_IO_RETRIES: usize = 12;
#[cfg(not(test))]
const FILE_ALLOCATION_MIN_BYTES: u64 = 1024 * 1024 * 1024;
#[cfg(test)]
const FILE_ALLOCATION_MIN_BYTES: u64 = u64::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RecoveryOutcome {
    Ready,
    AlreadyCommitted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SequentialStageState {
    schema_version: u32,
    job_id: String,
    game_id: String,
    install_path_fingerprint: String,
    from_version: String,
    target_version: String,
    plan_id: String,
    files: HashMap<String, SequentialFileState>,
    #[serde(default)]
    shared_chunk_remaining_uses: HashMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SequentialFileState {
    relative_path: String,
    path_hash: String,
    target_size: u64,
    target_sha256: String,
    #[serde(default)]
    preserve: bool,
    durable_chunks: usize,
    durable_bytes: u64,
    complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TransactionPhase {
    Prepared,
    FilesInstalled,
    MarkerCommitted,
    CleanupPending,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub(super) enum TransactionCommitProof {
    /// Backward-compatible proof used by update and fresh-install sessions.
    #[default]
    InstalledVersion,
    /// Repair keeps the same installed version, so its explicit durable
    /// transaction phase is the only valid commit proof.
    TransactionPhase,
    /// A patch is committed only after the marker records this exact ID.
    AppliedPatchId(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
enum TransactionFilePhase {
    Prepared,
    BackedUp,
    Installed,
    ObsoleteBackedUp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TransactionFile {
    relative_path: String,
    had_original: bool,
    obsolete: bool,
    #[serde(default)]
    metadata: bool,
    phase: TransactionFilePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTransaction {
    schema_version: u32,
    job_id: String,
    target_version: String,
    #[serde(default)]
    commit_proof: TransactionCommitProof,
    phase: TransactionPhase,
    files: Vec<TransactionFile>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct StageIoMetrics {
    pub(super) disk_read_bytes: u64,
    pub(super) disk_write_bytes: u64,
    pub(super) resume_rehash_bytes: u64,
    pub(super) sync_wait_ms: u64,
    pub(super) allocation_reserved_bytes: u64,
    pub(super) allocation_fallback_reason: Option<String>,
}

pub(super) struct SequentialFileWriter {
    path_hash: String,
    path: PathBuf,
    file: File,
    hasher: Sha256,
    next_chunk: usize,
    durable_bytes: u64,
    checkpointed_bytes: u64,
    uncheckpointed_bytes: u64,
    last_checkpoint: Instant,
    metrics: StageIoMetrics,
}

impl SequentialFileWriter {
    pub(super) fn next_chunk(&self) -> usize {
        self.next_chunk
    }

    #[cfg(test)]
    pub(super) fn checkpointed_bytes(&self) -> u64 {
        self.checkpointed_bytes
    }

    pub(super) fn metrics(&self) -> &StageIoMetrics {
        &self.metrics
    }

    pub(super) fn append(&mut self, chunk: &ChunkRef, data: &[u8]) -> Result<(), JobError> {
        if chunk.file_offset != self.durable_bytes {
            return Err(JobError::Depot(format!(
                "non-contiguous target chunk at offset {} (expected {})",
                chunk.file_offset, self.durable_bytes
            )));
        }
        verify_chunk_bytes(chunk, data)?;
        self.file
            .write_all(data)
            .map_err(|error| classify_file_io(error, &self.path, "append staging data"))?;
        self.hasher.update(data);
        self.next_chunk = self.next_chunk.saturating_add(1);
        self.durable_bytes = self.durable_bytes.saturating_add(data.len() as u64);
        self.uncheckpointed_bytes = self.uncheckpointed_bytes.saturating_add(data.len() as u64);
        self.metrics.disk_write_bytes = self
            .metrics
            .disk_write_bytes
            .saturating_add(data.len() as u64);
        Ok(())
    }

    pub(super) fn checkpoint_due(&self) -> bool {
        self.uncheckpointed_bytes >= DOWNLOAD_CHECKPOINT_BYTES
            || self.last_checkpoint.elapsed() >= CHECKPOINT_INTERVAL
    }
}

// Rollout name for the shared, verified staging primitive. Existing update
// code keeps its established type name while install/repair/patch use this
// alias, so old sessions remain pinned to the same schema and implementation.
pub(super) type VerifiedStageSession = SequentialUpdateSession;
pub(super) type VerifiedFileWriter = SequentialFileWriter;

pub(super) struct SequentialUpdateSession {
    downloading_root: PathBuf,
    install_root: PathBuf,
    session_root: PathBuf,
    stage_root: PathBuf,
    cache_root: PathBuf,
    state_path: PathBuf,
    transaction_path: PathBuf,
    job_key: String,
    state: SequentialStageState,
    transaction: UpdateTransaction,
    rollback_armed: bool,
    state_dirty: bool,
    state_bytes_since_persist: u64,
    last_state_persist: Instant,
}

impl SequentialUpdateSession {
    pub(super) fn open_owned_for_recovery(
        downloading_root: &Path,
        journal: &JobJournal,
        install_root: &Path,
    ) -> Result<Option<Self>, JobError> {
        let sessions_root = downloading_root.join(SESSION_DIRECTORY);
        for collision in 0_u8..32 {
            let owner = if collision == 0 {
                journal.id.clone()
            } else {
                format!("{}:{collision}", journal.id)
            };
            let job_key = short_stable_key(&owner);
            let session_root = sessions_root.join(&job_key);
            let state_path = session_root.join(STATE_FILE);
            let Some(state) = read_json_recovering::<SequentialStageState>(&state_path)? else {
                continue;
            };
            if state.job_id != journal.id {
                continue;
            }
            let expected_fingerprint = full_hash(&normalized_filesystem_path(install_root));
            let transaction_path = session_root.join(TRANSACTION_FILE);
            let Some(transaction) = read_json_recovering::<UpdateTransaction>(&transaction_path)?
            else {
                continue;
            };
            if transaction.phase == TransactionPhase::Complete {
                continue;
            }
            let expected_from_version = match &transaction.commit_proof {
                TransactionCommitProof::AppliedPatchId(_) => journal.to_version.as_str(),
                _ => journal.from_version.as_str(),
            };
            if state.schema_version != SEQUENTIAL_STAGE_SCHEMA
                || state.game_id != journal.game_id
                || state.install_path_fingerprint != expected_fingerprint
                || state.from_version != expected_from_version
                || state.target_version != journal.to_version
            {
                return Err(JobError::SessionMismatch(
                    "startup recovery ownership does not match the update journal".to_string(),
                ));
            }
            if transaction.job_id != journal.id || transaction.target_version != journal.to_version
            {
                return Err(JobError::SessionMismatch(
                    "startup recovery transaction belongs to another update".to_string(),
                ));
            }
            ensure_same_volume(&session_root, install_root)?;
            return Ok(Some(Self {
                downloading_root: downloading_root.to_path_buf(),
                install_root: install_root.to_path_buf(),
                stage_root: session_root.join(STAGE_DIRECTORY),
                cache_root: session_root.join(CACHE_DIRECTORY),
                state_path,
                transaction_path,
                session_root,
                job_key,
                state,
                transaction,
                rollback_armed: true,
                state_dirty: false,
                state_bytes_since_persist: 0,
                last_state_persist: Instant::now(),
            }));
        }
        Ok(None)
    }

    pub(super) fn prepare(
        downloading_root: &Path,
        journal: &JobJournal,
        from_version: &str,
        target_version: &str,
        install_root: &Path,
        files: &[FileEntry],
    ) -> Result<Self, JobError> {
        Self::prepare_with_commit_proof(
            downloading_root,
            journal,
            from_version,
            target_version,
            install_root,
            files,
            TransactionCommitProof::InstalledVersion,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_with_commit_proof(
        downloading_root: &Path,
        journal: &JobJournal,
        from_version: &str,
        target_version: &str,
        install_root: &Path,
        files: &[FileEntry],
        commit_proof: TransactionCommitProof,
    ) -> Result<Self, JobError> {
        let expected_state =
            new_stage_state(journal, from_version, target_version, install_root, files);
        let (job_key, session_root) = (0_u8..32)
            .find_map(|collision| {
                let owner = if collision == 0 {
                    journal.id.clone()
                } else {
                    format!("{}:{collision}", journal.id)
                };
                let key = short_stable_key(&owner);
                let root = downloading_root.join(SESSION_DIRECTORY).join(&key);
                let state_path = root.join(STATE_FILE);
                let transaction_path = root.join(TRANSACTION_FILE);
                let state_matches = match read_json_recovering::<SequentialStageState>(&state_path)
                {
                    Ok(Some(state)) => validate_owner(&state, &expected_state).is_ok(),
                    Ok(None) => true,
                    Err(_) => false,
                };
                let transaction_matches =
                    match read_json_recovering::<UpdateTransaction>(&transaction_path) {
                        Ok(Some(transaction)) => {
                            transaction.job_id == journal.id
                                && transaction.target_version == target_version
                                && transaction.commit_proof == commit_proof
                        }
                        Ok(None) => true,
                        Err(_) => false,
                    };
                (state_matches && transaction_matches).then_some((key, root))
            })
            .ok_or_else(|| {
                JobError::SessionMismatch(
                    "could not allocate an owned short staging session".to_string(),
                )
            })?;
        let stage_root = session_root.join(STAGE_DIRECTORY);
        let cache_root = session_root.join(CACHE_DIRECTORY);
        fs::create_dir_all(&stage_root)?;
        fs::create_dir_all(&cache_root)?;
        ensure_same_volume(&stage_root, install_root)?;

        let state_path = session_root.join(STATE_FILE);
        let transaction_path = session_root.join(TRANSACTION_FILE);
        let mut state = match read_json_recovering::<SequentialStageState>(&state_path)? {
            Some(state) => {
                validate_owner(&state, &expected_state)?;
                state
            }
            None => expected_state,
        };
        reconcile_shared_chunk_uses(&mut state, files);

        let transaction = match read_json_recovering::<UpdateTransaction>(&transaction_path)? {
            Some(transaction)
                if transaction.job_id == journal.id
                    && transaction.target_version == target_version =>
            {
                if transaction.commit_proof != commit_proof {
                    return Err(JobError::SessionMismatch(
                        "staging session commit proof changed".to_string(),
                    ));
                }
                transaction
            }
            Some(_) => {
                return Err(JobError::SessionMismatch(format!(
                    "{} belongs to another update",
                    session_root.display()
                )))
            }
            None => UpdateTransaction {
                schema_version: SEQUENTIAL_STAGE_SCHEMA,
                job_id: journal.id.clone(),
                target_version: target_version.to_string(),
                commit_proof,
                phase: TransactionPhase::Prepared,
                files: Vec::new(),
            },
        };

        let session = Self {
            downloading_root: downloading_root.to_path_buf(),
            install_root: install_root.to_path_buf(),
            session_root,
            stage_root,
            cache_root,
            state_path,
            transaction_path,
            job_key,
            state,
            transaction,
            rollback_armed: true,
            state_dirty: false,
            state_bytes_since_persist: 0,
            last_state_persist: Instant::now(),
        };
        session.persist_state()?;
        session.persist_transaction()?;
        session.validate_path_budget(install_root, files)?;
        Ok(session)
    }

    pub(super) fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    pub(super) fn stage_path(&self, file: &FileEntry) -> PathBuf {
        self.stage_path_for_hash(&manifest_path_hash(&file.path))
    }

    pub(super) fn durable_chunks(&self, file: &FileEntry) -> usize {
        self.state
            .files
            .get(&manifest_path_hash(&file.path))
            .map(|state| state.durable_chunks.min(file.chunks.len()))
            .unwrap_or(0)
    }

    pub(super) fn durable_bytes(&self, file: &FileEntry) -> u64 {
        self.state
            .files
            .get(&manifest_path_hash(&file.path))
            .map(|state| state.durable_bytes.min(file.size))
            .unwrap_or(0)
    }

    pub(super) fn total_durable_bytes(&self, files: &[FileEntry]) -> u64 {
        files.iter().map(|file| self.durable_bytes(file)).sum()
    }

    pub(super) fn remove_cached_chunk(&self, hash: &str) -> Result<(), JobError> {
        remove_file_if_exists_with_retry(&staged_chunk_path_from(&self.cache_root, hash))
    }

    pub(super) fn reconcile_chunk_references(
        &mut self,
        files: &[FileEntry],
    ) -> Result<(), JobError> {
        reconcile_shared_chunk_uses(&mut self.state, files);
        self.state_dirty = true;
        self.persist_state_now()
    }

    pub(super) fn release_checkpointed_chunks(
        &mut self,
        checkpointed_hashes: &mut Vec<String>,
    ) -> Result<(), JobError> {
        let mut removable = Vec::new();
        for hash in checkpointed_hashes.drain(..) {
            match self.state.shared_chunk_remaining_uses.get_mut(&hash) {
                Some(remaining) => {
                    *remaining = remaining.saturating_sub(1);
                    if *remaining == 0 {
                        self.state.shared_chunk_remaining_uses.remove(&hash);
                        removable.push(hash);
                    }
                }
                None => removable.push(hash),
            }
        }
        self.state_dirty = true;
        self.persist_state_if_due(false)?;
        for hash in removable {
            self.remove_cached_chunk(&hash)?;
        }
        Ok(())
    }

    pub(super) fn recover(
        &mut self,
        install_root: &Path,
        target_version: &str,
    ) -> Result<RecoveryOutcome, JobError> {
        if self.transaction.files.is_empty() {
            return Ok(RecoveryOutcome::Ready);
        }

        let marker = read_install_marker(install_root)?;
        let marker_committed = match &self.transaction.commit_proof {
            TransactionCommitProof::InstalledVersion => marker
                .as_ref()
                .and_then(|marker| usable_installed_version(&marker.version))
                .is_some_and(|version| version == target_version),
            TransactionCommitProof::TransactionPhase => matches!(
                self.transaction.phase,
                TransactionPhase::MarkerCommitted
                    | TransactionPhase::CleanupPending
                    | TransactionPhase::Complete
            ),
            TransactionCommitProof::AppliedPatchId(expected) => marker
                .as_ref()
                .and_then(|marker| marker.applied_patch_id.as_deref())
                .is_some_and(|actual| actual == expected),
        };

        if marker_committed {
            // The install marker is the commit point. Any later transaction or
            // cleanup failure must leave the target files installed.
            self.rollback_armed = false;
            for file in self.state.files.values() {
                let target = safe_join(install_root, &file.relative_path).ok_or_else(|| {
                    JobError::Depot(format!("unsafe manifest path: {}", file.relative_path))
                })?;
                let entry = FileEntry {
                    path: file.relative_path.clone(),
                    size: file.target_size,
                    sha256: file.target_sha256.clone(),
                    chunks: Vec::new(),
                    executable: false,
                    delta_patches: None,
                    preserve: false,
                };
                if !(file.preserve && long_path(&target).is_file())
                    && !target_file_valid(&long_path(&target), &entry)?
                {
                    return Err(JobError::Depot(format!(
                        "committed update file is invalid: {}",
                        file.relative_path
                    )));
                }
            }
            for record in self
                .transaction
                .files
                .iter()
                .filter(|record| record.metadata)
            {
                let target = safe_join(install_root, &record.relative_path).ok_or_else(|| {
                    JobError::Depot(format!(
                        "unsafe committed metadata path: {}",
                        record.relative_path
                    ))
                })?;
                if !long_path(&target).is_file() {
                    return Err(JobError::Depot(format!(
                        "committed transaction metadata is missing: {}",
                        record.relative_path
                    )));
                }
            }
            self.transaction.phase = TransactionPhase::CleanupPending;
            self.persist_transaction()?;
            self.cleanup_backups(install_root)?;
            return Ok(RecoveryOutcome::AlreadyCommitted);
        }

        self.rollback(install_root)?;
        self.reset_staging_after_rollback()?;
        Ok(RecoveryOutcome::Ready)
    }

    pub(super) fn open_writer(
        &mut self,
        file: &FileEntry,
    ) -> Result<SequentialFileWriter, JobError> {
        if !long_path(&self.session_root).is_dir() {
            return Err(JobError::SessionMissing(
                self.session_root.display().to_string(),
            ));
        }
        if !long_path(&self.stage_root).is_dir() {
            return Err(JobError::StageMissing(
                self.stage_root.display().to_string(),
            ));
        }
        let path_hash = manifest_path_hash(&file.path);
        let stage_path = self.stage_path_for_hash(&path_hash);
        let lp_stage = long_path(&stage_path);
        let snapshot =
            self.state.files.get(&path_hash).cloned().ok_or_else(|| {
                JobError::Depot(format!("missing staging state for {}", file.path))
            })?;

        let mut handle = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lp_stage)
            .map_err(|error| classify_file_io(error, &stage_path, "open staging file"))?;
        let actual_len = handle.metadata()?.len();
        let mut durable_chunks = snapshot.durable_chunks.min(file.chunks.len());
        let mut durable_bytes = snapshot.durable_bytes.min(file.size);

        if actual_len < durable_bytes {
            durable_chunks = 0;
            durable_bytes = 0;
        } else if actual_len > durable_bytes {
            handle
                .set_len(durable_bytes)
                .map_err(|error| classify_file_io(error, &stage_path, "truncate staging file"))?;
        }

        handle.seek(SeekFrom::Start(0))?;
        let mut hasher = Sha256::new();
        let mut verified_chunks = 0_usize;
        let mut verified_bytes = 0_u64;
        for chunk in file.chunks.iter().take(durable_chunks) {
            if chunk.file_offset != verified_bytes {
                break;
            }
            let mut bytes = vec![0_u8; chunk.uncompressed_size as usize];
            if handle.read_exact(&mut bytes).is_err() || verify_chunk_bytes(chunk, &bytes).is_err()
            {
                break;
            }
            hasher.update(&bytes);
            verified_chunks = verified_chunks.saturating_add(1);
            verified_bytes = verified_bytes.saturating_add(bytes.len() as u64);
        }
        if verified_chunks != durable_chunks || verified_bytes != durable_bytes {
            durable_chunks = verified_chunks;
            durable_bytes = verified_bytes;
            handle.set_len(durable_bytes).map_err(|error| {
                classify_file_io(error, &stage_path, "repair staging checkpoint")
            })?;
            self.update_progress(&path_hash, durable_chunks, durable_bytes, false)?;
            self.persist_state_now()?;
        }

        let allocation =
            try_reserve_file_allocation(&handle, &stage_path, file.size, FILE_ALLOCATION_MIN_BYTES);

        handle.seek(SeekFrom::Start(durable_bytes))?;
        Ok(SequentialFileWriter {
            path_hash,
            path: stage_path,
            file: handle,
            hasher,
            next_chunk: durable_chunks,
            durable_bytes,
            checkpointed_bytes: durable_bytes,
            uncheckpointed_bytes: 0,
            last_checkpoint: Instant::now(),
            metrics: StageIoMetrics {
                disk_read_bytes: verified_bytes,
                resume_rehash_bytes: verified_bytes,
                allocation_reserved_bytes: allocation.reserved_bytes,
                allocation_fallback_reason: allocation.fallback_reason,
                ..StageIoMetrics::default()
            },
        })
    }

    pub(super) fn checkpoint_writer(
        &mut self,
        writer: &mut SequentialFileWriter,
        complete: bool,
    ) -> Result<(), JobError> {
        let sync_started = Instant::now();
        writer
            .file
            .sync_data()
            .map_err(|error| classify_file_io(error, &writer.path, "checkpoint staging file"))?;
        writer.metrics.sync_wait_ms = writer
            .metrics
            .sync_wait_ms
            .saturating_add(sync_started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        self.update_progress(
            &writer.path_hash,
            writer.next_chunk,
            writer.durable_bytes,
            complete,
        )?;
        // checkpoint_writer is an explicit crash-resume boundary. Persist its
        // metadata now; finish_writer uses the debounced path and is made
        // durable in batches immediately before commit.
        self.persist_state_now()?;
        writer.checkpointed_bytes = writer.durable_bytes;
        writer.uncheckpointed_bytes = 0;
        writer.last_checkpoint = Instant::now();
        Ok(())
    }

    pub(super) fn finish_writer(
        &mut self,
        writer: &mut SequentialFileWriter,
        file: &FileEntry,
    ) -> Result<(), JobError> {
        if writer.next_chunk != file.chunks.len() || writer.durable_bytes != file.size {
            return Err(JobError::Depot(format!(
                "staging file is incomplete: {}",
                file.path
            )));
        }
        writer
            .file
            .flush()
            .map_err(|error| classify_file_io(error, &writer.path, "flush staging file"))?;
        writer.uncheckpointed_bytes = 0;
        writer.last_checkpoint = Instant::now();
        let actual = hex::encode(writer.hasher.clone().finalize());
        if actual != file.sha256 {
            return Err(JobError::Depot(format!(
                "staging hash mismatch: {}",
                file.path
            )));
        }
        let sync_started = Instant::now();
        writer
            .file
            .sync_data()
            .map_err(|error| classify_file_io(error, &writer.path, "finalize staging file"))?;
        writer.metrics.sync_wait_ms = writer
            .metrics
            .sync_wait_ms
            .saturating_add(sync_started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64);
        self.update_progress(
            &writer.path_hash,
            writer.next_chunk,
            writer.durable_bytes,
            true,
        )?;
        self.persist_state_now()?;
        writer.checkpointed_bytes = writer.durable_bytes;
        Ok(())
    }

    pub(super) fn sync_staged_files(&self, files: &[FileEntry]) -> Result<(), JobError> {
        if files.is_empty() {
            return Ok(());
        }
        let pending = files
            .iter()
            .filter(|file| {
                !self
                    .state
                    .files
                    .get(&manifest_path_hash(&file.path))
                    .is_some_and(|state| state.complete)
            })
            .map(|file| self.stage_path(file))
            .collect::<Vec<_>>();
        if pending.is_empty() {
            return Ok(());
        }
        let queue = Arc::new(Mutex::new(VecDeque::from(pending)));
        let (tx, rx) = mpsc::channel::<Result<(), (PathBuf, std::io::Error)>>();
        let worker_count = files.len().min(STAGE_SYNC_WORKERS).max(1);

        thread::scope(|scope| {
            for _ in 0..worker_count {
                let queue = Arc::clone(&queue);
                let tx = tx.clone();
                scope.spawn(move || loop {
                    let path = {
                        let mut guard = queue.lock().expect("stage sync queue poisoned");
                        guard.pop_front()
                    };
                    let Some(path) = path else {
                        break;
                    };
                    let result = OpenOptions::new()
                        .read(true)
                        .write(true)
                        .open(long_path(&path))
                        .and_then(|file| file.sync_data())
                        .map_err(|error| (path, error));
                    if tx.send(result).is_err() {
                        break;
                    }
                });
            }
            drop(tx);
            for result in rx {
                if let Err((path, error)) = result {
                    return Err(classify_file_io(error, &path, "sync staged file"));
                }
            }
            Ok(())
        })
    }

    pub(super) fn prepare_commit_batch(
        &mut self,
        install_root: &Path,
        files: &[FileEntry],
    ) -> Result<(), JobError> {
        let mut pending = Vec::new();
        for file in files {
            if self.transaction.files.iter().any(|record| {
                !record.obsolete
                    && !record.metadata
                    && record.relative_path.eq_ignore_ascii_case(&file.path)
            }) {
                continue;
            }
            let target = safe_join(install_root, &file.path)
                .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
            let backup = transaction_backup_path(&target, &self.job_key)?;
            if long_path(&backup).exists() {
                return Err(JobError::SessionMismatch(format!(
                    "unexpected transaction backup already exists for {}",
                    file.path
                )));
            }
            pending.push(TransactionFile {
                relative_path: file.path.clone(),
                had_original: long_path(&target).exists(),
                obsolete: false,
                metadata: false,
                phase: TransactionFilePhase::Prepared,
            });
        }
        if pending.is_empty() {
            return Ok(());
        }
        self.transaction.files.extend(pending);
        self.persist_transaction()
    }

    pub(super) fn prepare_obsolete_batch(
        &mut self,
        install_root: &Path,
        relative_paths: &[String],
    ) -> Result<(), JobError> {
        let mut pending = Vec::new();
        for relative_path in relative_paths {
            if self.transaction.files.iter().any(|record| {
                record.obsolete && record.relative_path.eq_ignore_ascii_case(relative_path)
            }) {
                continue;
            }
            let target = safe_join(install_root, relative_path)
                .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {relative_path}")))?;
            if !long_path(&target).exists() {
                continue;
            }
            let backup = transaction_backup_path(&target, &self.job_key)?;
            if long_path(&backup).exists() {
                return Err(JobError::SessionMismatch(format!(
                    "unexpected obsolete backup already exists for {relative_path}"
                )));
            }
            pending.push(TransactionFile {
                relative_path: relative_path.clone(),
                had_original: true,
                obsolete: true,
                metadata: false,
                phase: TransactionFilePhase::Prepared,
            });
        }
        if pending.is_empty() {
            return Ok(());
        }
        self.transaction.files.extend(pending);
        self.persist_transaction()
    }

    pub(super) fn commit_file(
        &mut self,
        install_root: &Path,
        file: &FileEntry,
    ) -> Result<Option<(PathBuf, PathBuf)>, JobError> {
        if !self.transaction.files.iter().any(|record| {
            !record.obsolete
                && !record.metadata
                && record.relative_path.eq_ignore_ascii_case(&file.path)
        }) {
            self.prepare_commit_batch(install_root, std::slice::from_ref(file))?;
        }
        let path_hash = manifest_path_hash(&file.path);
        let stage = self.stage_path_for_hash(&path_hash);
        let target = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(&long_path(parent))?;
        }
        let backup = transaction_backup_path(&target, &self.job_key)?;
        let had_original = self
            .transaction
            .files
            .iter()
            .rev()
            .find(|record| {
                !record.obsolete
                    && !record.metadata
                    && record.relative_path.eq_ignore_ascii_case(&file.path)
            })
            .map(|record| record.had_original)
            .ok_or_else(|| {
                JobError::Depot(format!("missing transaction intent for {}", file.path))
            })?;

        if long_path(&backup).exists() {
            return Err(JobError::SessionMismatch(format!(
                "unexpected transaction backup already exists for {}",
                file.path
            )));
        }

        if had_original {
            if !long_path(&target).exists() {
                return Err(JobError::SessionMismatch(format!(
                    "installed file disappeared before commit: {}",
                    file.path
                )));
            }
            rename_with_retry(&target, &backup, "back up installed file")?;
            self.set_transaction_file_phase_in_memory(&file.path, TransactionFilePhase::BackedUp)?;
        } else if long_path(&target).exists() {
            return Err(JobError::SessionMismatch(format!(
                "new target appeared before commit: {}",
                file.path
            )));
        }

        if let Err(error) = rename_with_retry(&stage, &target, "install verified staging file") {
            if had_original && long_path(&backup).exists() {
                let _ = rename_with_retry(&backup, &target, "restore installed file");
            }
            return Err(error);
        }
        self.set_transaction_file_phase_in_memory(&file.path, TransactionFilePhase::Installed)?;
        self.transaction.phase = TransactionPhase::FilesInstalled;

        Ok(had_original.then_some((target, backup)))
    }

    /// Persist rollback ownership before moving metadata out of the way. The
    /// caller may then atomically write the new metadata; recovery restores
    /// the previous file if the job-specific commit proof was not reached.
    pub(super) fn backup_metadata_file(
        &mut self,
        install_root: &Path,
        relative_path: &str,
    ) -> Result<(), JobError> {
        if self.transaction.files.iter().any(|record| {
            record.metadata && record.relative_path.eq_ignore_ascii_case(relative_path)
        }) {
            return Ok(());
        }
        let target = safe_join(install_root, relative_path)
            .ok_or_else(|| JobError::Depot(format!("unsafe metadata path: {relative_path}")))?;
        let had_original = long_path(&target).exists();
        let backup = transaction_backup_path(&target, &self.job_key)?;
        if long_path(&backup).exists() {
            return Err(JobError::SessionMismatch(format!(
                "unexpected metadata backup already exists for {relative_path}"
            )));
        }
        self.transaction.files.push(TransactionFile {
            relative_path: relative_path.to_string(),
            had_original,
            obsolete: false,
            metadata: true,
            phase: TransactionFilePhase::Prepared,
        });
        self.persist_transaction()?;
        if had_original {
            rename_with_retry(&target, &backup, "back up transaction metadata")?;
            self.set_transaction_file_phase_in_memory(
                relative_path,
                TransactionFilePhase::BackedUp,
            )?;
        }
        Ok(())
    }

    pub(super) fn backup_obsolete_file(
        &mut self,
        install_root: &Path,
        relative_path: &str,
    ) -> Result<(), JobError> {
        let target = safe_join(install_root, relative_path)
            .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {relative_path}")))?;
        if !long_path(&target).exists() {
            return Ok(());
        }
        if !self.transaction.files.iter().any(|record| {
            record.obsolete && record.relative_path.eq_ignore_ascii_case(relative_path)
        }) {
            self.prepare_obsolete_batch(install_root, &[relative_path.to_string()])?;
        }
        let backup = transaction_backup_path(&target, &self.job_key)?;
        if long_path(&backup).exists() {
            return Err(JobError::SessionMismatch(format!(
                "unexpected obsolete backup already exists for {relative_path}"
            )));
        }
        rename_with_retry(&target, &backup, "back up obsolete file")?;
        self.set_transaction_file_phase_in_memory(
            relative_path,
            TransactionFilePhase::ObsoleteBackedUp,
        )
    }

    pub(super) fn mark_marker_committed(&mut self) -> Result<(), JobError> {
        let previous_phase = self.transaction.phase.clone();
        self.transaction.phase = TransactionPhase::MarkerCommitted;
        if self.transaction.commit_proof == TransactionCommitProof::TransactionPhase {
            // For repair, txn.json is itself the durable commit proof. Keep
            // rollback armed until that proof reaches disk and can be read
            // back. A failed write must not leave only the in-memory phase
            // looking committed to Drop.
            match self.persist_transaction() {
                Ok(()) => self.rollback_armed = false,
                Err(error) => {
                    let proof_is_durable =
                        read_json_recovering::<UpdateTransaction>(&self.transaction_path)
                            .ok()
                            .flatten()
                            .is_some_and(|transaction| {
                                transaction.job_id == self.transaction.job_id
                                    && transaction.phase == TransactionPhase::MarkerCommitted
                            });
                    if proof_is_durable {
                        self.rollback_armed = false;
                    } else {
                        self.transaction.phase = previous_phase;
                        return Err(error);
                    }
                }
            }
        } else {
            // Install/update/patch already wrote an independently durable
            // marker. Never roll their files back solely because persisting
            // the advisory transaction phase failed afterward.
            self.rollback_armed = false;
            self.persist_transaction()?;
        }
        Ok(())
    }

    pub(super) fn cleanup_committed(&mut self, install_root: &Path) -> Result<(), JobError> {
        self.transaction.phase = TransactionPhase::CleanupPending;
        self.persist_transaction()?;
        self.cleanup_backups(install_root)
    }

    pub(super) fn cleanup_session_files(&self) -> Result<(), JobError> {
        remove_flat_files(&self.cache_root)?;
        remove_flat_files(&self.stage_root)?;
        remove_file_if_exists(&self.state_path)?;
        remove_file_if_exists(&self.transaction_path)?;
        remove_file_if_exists(&recovery_path(&self.state_path))?;
        remove_file_if_exists(&recovery_path(&self.transaction_path))?;
        remove_empty_dir(&self.cache_root)?;
        remove_empty_dir(&self.stage_root)?;
        remove_empty_dir(&self.session_root)?;
        if let Some(session_parent) = self.session_root.parent() {
            remove_empty_dir(session_parent)?;
        }
        remove_empty_dir(&self.downloading_root)?;
        Ok(())
    }

    pub(super) fn cleanup_owned_session(
        downloading_root: &Path,
        job_id: &str,
    ) -> Result<(), JobError> {
        let sessions_root = downloading_root.join(SESSION_DIRECTORY);
        for collision in 0_u8..32 {
            let owner = if collision == 0 {
                job_id.to_string()
            } else {
                format!("{job_id}:{collision}")
            };
            let session_root = sessions_root.join(short_stable_key(&owner));
            let state_path = session_root.join(STATE_FILE);
            let Some(state) = read_json_recovering::<SequentialStageState>(&state_path)? else {
                continue;
            };
            if state.job_id != job_id {
                continue;
            }
            let cache_root = session_root.join(CACHE_DIRECTORY);
            let stage_root = session_root.join(STAGE_DIRECTORY);
            remove_flat_files(&cache_root)?;
            remove_flat_files(&stage_root)?;
            for path in [
                state_path.clone(),
                session_root.join(TRANSACTION_FILE),
                recovery_path(&state_path),
                recovery_path(&session_root.join(TRANSACTION_FILE)),
            ] {
                remove_file_if_exists(&path)?;
            }
            remove_empty_dir(&cache_root)?;
            remove_empty_dir(&stage_root)?;
            remove_empty_dir(&session_root)?;
        }
        remove_empty_dir(&sessions_root)?;
        remove_empty_dir(downloading_root)?;
        Ok(())
    }

    pub(super) fn has_pending_transaction(
        downloading_root: &Path,
        job_id: &str,
    ) -> Result<bool, JobError> {
        let sessions_root = downloading_root.join(SESSION_DIRECTORY);
        for collision in 0_u8..32 {
            let owner = if collision == 0 {
                job_id.to_string()
            } else {
                format!("{job_id}:{collision}")
            };
            let transaction_path = sessions_root
                .join(short_stable_key(&owner))
                .join(TRANSACTION_FILE);
            let Some(transaction) = read_json_recovering::<UpdateTransaction>(&transaction_path)?
            else {
                continue;
            };
            if transaction.job_id == job_id
                && !transaction.files.is_empty()
                && transaction.phase != TransactionPhase::Complete
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn cleanup_backups(&mut self, install_root: &Path) -> Result<(), JobError> {
        for record in &self.transaction.files {
            let target = safe_join(install_root, &record.relative_path).ok_or_else(|| {
                JobError::Depot(format!("unsafe manifest path: {}", record.relative_path))
            })?;
            let backup = transaction_backup_path(&target, &self.job_key)?;
            remove_file_if_exists_with_retry(&backup)?;
        }
        self.transaction.phase = TransactionPhase::Complete;
        self.persist_transaction()
    }

    fn rollback(&mut self, install_root: &Path) -> Result<(), JobError> {
        for record in self.transaction.files.iter().rev() {
            let target = safe_join(install_root, &record.relative_path).ok_or_else(|| {
                JobError::Depot(format!("unsafe manifest path: {}", record.relative_path))
            })?;
            let backup = transaction_backup_path(&target, &self.job_key)?;
            let backup_exists = long_path(&backup).exists();
            let target_exists = long_path(&target).exists();

            if backup_exists {
                if target_exists {
                    remove_file_if_exists_with_retry(&target)?;
                }
                rename_with_retry(&backup, &target, "roll back installed file")?;
            } else if !record.had_original && target_exists {
                // The transaction record is durable before stage -> target.
                // A crash can therefore leave a newly-created target behind
                // while its phase is still Prepared. Ownership, rather than
                // the advisory phase, determines whether rollback removes it.
                remove_file_if_exists_with_retry(&target)?;
            }
        }
        self.transaction.files.clear();
        self.transaction.phase = TransactionPhase::Prepared;
        self.persist_transaction()
    }

    fn reset_staging_after_rollback(&mut self) -> Result<(), JobError> {
        remove_flat_files(&self.stage_root)?;
        for file in self.state.files.values_mut() {
            file.durable_chunks = 0;
            file.durable_bytes = 0;
            file.complete = false;
        }
        self.state_dirty = true;
        self.persist_state_now()
    }

    fn set_transaction_file_phase_in_memory(
        &mut self,
        relative_path: &str,
        phase: TransactionFilePhase,
    ) -> Result<(), JobError> {
        let record = self
            .transaction
            .files
            .iter_mut()
            .rev()
            .find(|record| record.relative_path.eq_ignore_ascii_case(relative_path))
            .ok_or_else(|| {
                JobError::Depot(format!("missing transaction record for {relative_path}"))
            })?;
        record.phase = phase;
        Ok(())
    }

    fn update_progress(
        &mut self,
        path_hash: &str,
        durable_chunks: usize,
        durable_bytes: u64,
        complete: bool,
    ) -> Result<(), JobError> {
        let previous_bytes = {
            let state = self.state.files.get_mut(path_hash).ok_or_else(|| {
                JobError::Depot(format!("missing sequential progress for {path_hash}"))
            })?;
            let previous_bytes = state.durable_bytes;
            state.durable_chunks = durable_chunks;
            state.durable_bytes = durable_bytes;
            state.complete = complete;
            previous_bytes
        };
        self.state_dirty = true;
        self.state_bytes_since_persist = self
            .state_bytes_since_persist
            .saturating_add(durable_bytes.saturating_sub(previous_bytes));
        Ok(())
    }

    fn persist_state(&self) -> Result<(), JobError> {
        write_json_atomic(&self.state_path, &self.state)
    }

    fn persist_state_if_due(&mut self, force: bool) -> Result<(), JobError> {
        if !self.state_dirty {
            return Ok(());
        }
        if !force
            && self.state_bytes_since_persist < STATE_PERSIST_BYTES
            && self.last_state_persist.elapsed() < STATE_PERSIST_INTERVAL
        {
            return Ok(());
        }
        self.persist_state_now()
    }

    fn persist_state_now(&mut self) -> Result<(), JobError> {
        self.persist_state()?;
        self.state_dirty = false;
        self.state_bytes_since_persist = 0;
        self.last_state_persist = Instant::now();
        Ok(())
    }

    fn persist_transaction(&self) -> Result<(), JobError> {
        write_json_atomic(&self.transaction_path, &self.transaction)
    }

    fn stage_path_for_hash(&self, path_hash: &str) -> PathBuf {
        self.stage_root.join(format!("{path_hash}.stage"))
    }

    fn validate_path_budget(
        &self,
        install_root: &Path,
        files: &[FileEntry],
    ) -> Result<(), JobError> {
        let stage_chars = self
            .stage_root
            .join(format!("{}.stage", "f".repeat(32)))
            .to_string_lossy()
            .encode_utf16()
            .count();
        if stage_chars >= 32_000 {
            return Err(JobError::Depot(format!(
                "staging path is too long: {}",
                self.stage_root.display()
            )));
        }
        for file in files {
            let target = safe_join(install_root, &file.path)
                .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
            if target.to_string_lossy().encode_utf16().count() >= 32_000 {
                return Err(JobError::Depot(format!(
                    "install target exceeds the Windows extended path limit: {}",
                    file.path
                )));
            }
        }
        Ok(())
    }
}

impl Drop for SequentialUpdateSession {
    fn drop(&mut self) {
        if self.rollback_armed && !self.transaction.files.is_empty() {
            let marker = read_install_marker(&self.install_root).ok().flatten();
            let committed = match &self.transaction.commit_proof {
                TransactionCommitProof::InstalledVersion => marker
                    .as_ref()
                    .and_then(|marker| usable_installed_version(&marker.version))
                    .is_some_and(|version| version == self.transaction.target_version),
                TransactionCommitProof::TransactionPhase => matches!(
                    self.transaction.phase,
                    TransactionPhase::MarkerCommitted
                        | TransactionPhase::CleanupPending
                        | TransactionPhase::Complete
                ),
                TransactionCommitProof::AppliedPatchId(expected) => marker
                    .as_ref()
                    .and_then(|marker| marker.applied_patch_id.as_deref())
                    .is_some_and(|actual| actual == expected),
            };
            if committed {
                return;
            }
            let install_root = self.install_root.clone();
            if let Err(error) = self.rollback(&install_root) {
                eprintln!("[UPDATE] Transaction rollback remains pending: {error}");
            }
        }
    }
}

fn new_stage_state(
    journal: &JobJournal,
    from_version: &str,
    target_version: &str,
    install_root: &Path,
    files: &[FileEntry],
) -> SequentialStageState {
    let mut states = HashMap::new();
    for file in files {
        let path_hash = manifest_path_hash(&file.path);
        states.insert(
            path_hash.clone(),
            SequentialFileState {
                relative_path: file.path.clone(),
                path_hash,
                target_size: file.size,
                target_sha256: file.sha256.clone(),
                preserve: file.preserve,
                durable_chunks: 0,
                durable_bytes: 0,
                complete: false,
            },
        );
    }
    SequentialStageState {
        schema_version: SEQUENTIAL_STAGE_SCHEMA,
        job_id: journal.id.clone(),
        game_id: journal.game_id.clone(),
        install_path_fingerprint: full_hash(&normalized_filesystem_path(install_root)),
        from_version: from_version.to_string(),
        target_version: target_version.to_string(),
        plan_id: sequential_plan_id(files),
        files: states,
        shared_chunk_remaining_uses: HashMap::new(),
    }
}

fn reconcile_shared_chunk_uses(state: &mut SequentialStageState, files: &[FileEntry]) {
    let mut total_uses = HashMap::<String, usize>::new();
    let mut remaining_uses = HashMap::<String, usize>::new();
    for file in files {
        for chunk in &file.chunks {
            *total_uses.entry(chunk.hash.clone()).or_default() += 1;
        }
        let durable = state
            .files
            .get(&manifest_path_hash(&file.path))
            .map(|snapshot| snapshot.durable_chunks.min(file.chunks.len()))
            .unwrap_or(0);
        for chunk in file.chunks.iter().skip(durable) {
            *remaining_uses.entry(chunk.hash.clone()).or_default() += 1;
        }
    }
    remaining_uses.retain(|hash, remaining| {
        *remaining > 0 && total_uses.get(hash).copied().unwrap_or_default() > 1
    });
    state.shared_chunk_remaining_uses = remaining_uses;
}

fn validate_owner(
    actual: &SequentialStageState,
    expected: &SequentialStageState,
) -> Result<(), JobError> {
    if actual.schema_version != SEQUENTIAL_STAGE_SCHEMA
        || actual.job_id != expected.job_id
        || actual.game_id != expected.game_id
        || actual.install_path_fingerprint != expected.install_path_fingerprint
        || actual.from_version != expected.from_version
        || actual.target_version != expected.target_version
        || actual.plan_id != expected.plan_id
    {
        return Err(JobError::SessionMismatch(
            "session ownership or update plan changed".to_string(),
        ));
    }
    Ok(())
}

fn sequential_plan_id(files: &[FileEntry]) -> String {
    let mut sorted = files.iter().collect::<Vec<_>>();
    sorted.sort_by_key(|file| normalize_manifest_path(&file.path));
    let mut hasher = Sha256::new();
    for file in sorted {
        hasher.update(normalize_manifest_path(&file.path).as_bytes());
        hasher.update(file.size.to_le_bytes());
        hasher.update(file.sha256.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn manifest_path_hash(path: &str) -> String {
    full_hash(&normalize_manifest_path(path))[..32].to_string()
}

fn normalize_manifest_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn normalized_filesystem_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
}

fn full_hash(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn transaction_backup_path(target: &Path, job_key: &str) -> Result<PathBuf, JobError> {
    sibling_path(target, &format!("007launcher.bak.{job_key}"))
}

#[derive(Debug, Default)]
struct AllocationOutcome {
    reserved_bytes: u64,
    fallback_reason: Option<String>,
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct WindowsVolumeInfo {
    root: String,
    filesystem: String,
    remote: bool,
}

#[cfg(target_os = "windows")]
fn windows_volume_info(path: &Path) -> Result<WindowsVolumeInfo, std::io::Error> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use winapi::um::fileapi::{GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW};
    use winapi::um::winbase::DRIVE_REMOTE;

    let path = long_path(path);
    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut root = vec![0_u16; 32_768];
    let root_ok =
        unsafe { GetVolumePathNameW(path_wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) };
    if root_ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let root_len = root
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(root.len());
    root.truncate(root_len.saturating_add(1));

    let mut filesystem = vec![0_u16; 64];
    let info_ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    };
    if info_ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let fs_len = filesystem
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(filesystem.len());
    let root_text = String::from_utf16_lossy(&root[..root.len().saturating_sub(1)]);
    let filesystem = String::from_utf16_lossy(&filesystem[..fs_len]);
    let drive_type = unsafe { GetDriveTypeW(root.as_ptr()) };
    Ok(WindowsVolumeInfo {
        root: root_text.to_ascii_lowercase(),
        filesystem,
        remote: drive_type == DRIVE_REMOTE,
    })
}

#[cfg(target_os = "windows")]
fn ensure_same_volume(stage_root: &Path, install_root: &Path) -> Result<(), JobError> {
    let stage = windows_volume_info(stage_root).map_err(|error| {
        JobError::Depot(format!(
            "cannot identify staging volume '{}': {error}",
            stage_root.display()
        ))
    })?;
    let install = windows_volume_info(install_root).map_err(|error| {
        JobError::Depot(format!(
            "cannot identify install volume '{}': {error}",
            install_root.display()
        ))
    })?;
    if stage.root != install.root {
        return Err(JobError::Depot(format!(
            "verified staging and install target must be on the same volume ({} != {})",
            stage.root, install.root
        )));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn ensure_same_volume(_stage_root: &Path, _install_root: &Path) -> Result<(), JobError> {
    Ok(())
}

#[cfg(target_os = "windows")]
fn try_reserve_file_allocation(
    file: &File,
    path: &Path,
    target_size: u64,
    minimum_size: u64,
) -> AllocationOutcome {
    use std::mem::{size_of, zeroed};
    use std::os::windows::io::AsRawHandle;
    use winapi::um::fileapi::{SetFileInformationByHandle, FILE_ALLOCATION_INFO};
    use winapi::um::minwinbase::FileAllocationInfo;
    use winapi::um::winnt::HANDLE;

    if target_size < minimum_size || target_size > i64::MAX as u64 {
        return AllocationOutcome::default();
    }
    let volume = match windows_volume_info(path) {
        Ok(volume) => volume,
        Err(error) => {
            return AllocationOutcome {
                fallback_reason: Some(format!("volume query failed: {error}")),
                ..AllocationOutcome::default()
            }
        }
    };
    if volume.remote {
        return AllocationOutcome {
            fallback_reason: Some("network volume".to_string()),
            ..AllocationOutcome::default()
        };
    }
    if !volume.filesystem.eq_ignore_ascii_case("NTFS")
        && !volume.filesystem.eq_ignore_ascii_case("ReFS")
    {
        return AllocationOutcome {
            fallback_reason: Some(format!("unsupported filesystem: {}", volume.filesystem)),
            ..AllocationOutcome::default()
        };
    }

    let original_eof = match file.metadata() {
        Ok(metadata) => metadata.len(),
        Err(error) => {
            return AllocationOutcome {
                fallback_reason: Some(format!("metadata query failed: {error}")),
                ..AllocationOutcome::default()
            }
        }
    };
    let mut info: FILE_ALLOCATION_INFO = unsafe { zeroed() };
    unsafe {
        *info.AllocationSize.QuadPart_mut() = target_size as i64;
    }
    let result = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as HANDLE,
            FileAllocationInfo,
            &mut info as *mut FILE_ALLOCATION_INFO as *mut _,
            size_of::<FILE_ALLOCATION_INFO>() as u32,
        )
    };
    if result == 0 {
        return AllocationOutcome {
            fallback_reason: Some(format!(
                "FileAllocationInfo failed: {}",
                std::io::Error::last_os_error()
            )),
            ..AllocationOutcome::default()
        };
    }
    match file.metadata() {
        Ok(metadata) if metadata.len() == original_eof => AllocationOutcome {
            reserved_bytes: target_size.saturating_sub(original_eof),
            fallback_reason: None,
        },
        Ok(metadata) => {
            let observed = metadata.len();
            let _ = file.set_len(original_eof);
            AllocationOutcome {
                fallback_reason: Some(format!(
                    "allocation unexpectedly changed EOF from {original_eof} to {observed}"
                )),
                ..AllocationOutcome::default()
            }
        }
        Err(error) => AllocationOutcome {
            fallback_reason: Some(format!("EOF verification failed: {error}")),
            ..AllocationOutcome::default()
        },
    }
}

#[cfg(not(target_os = "windows"))]
fn try_reserve_file_allocation(
    _file: &File,
    _path: &Path,
    target_size: u64,
    minimum_size: u64,
) -> AllocationOutcome {
    if target_size >= minimum_size {
        AllocationOutcome {
            fallback_reason: Some("allocation hint is only available on Windows".to_string()),
            ..AllocationOutcome::default()
        }
    } else {
        AllocationOutcome::default()
    }
}

fn classify_file_io(error: std::io::Error, path: &Path, operation: &str) -> JobError {
    match error.raw_os_error() {
        Some(2 | 3) => JobError::StageMissing(format!("{operation}: {}", path.display())),
        Some(5 | 32 | 33) => JobError::FileLocked(format!("{operation}: {}", path.display())),
        Some(112) => JobError::DiskFull(format!("{operation}: {}", path.display())),
        _ => JobError::Io(error),
    }
}

fn rename_with_retry(from: &Path, to: &Path, operation: &str) -> Result<(), JobError> {
    let lp_from = long_path(from);
    let lp_to = long_path(to);
    for attempt in 0..FILE_IO_RETRIES {
        match fs::rename(&lp_from, &lp_to) {
            Ok(()) => return Ok(()),
            Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) => {
                if attempt + 1 == FILE_IO_RETRIES {
                    return Err(JobError::FileLocked(format!(
                        "{operation}: {}",
                        from.display()
                    )));
                }
                let delay = 100_u64.saturating_mul(1_u64 << attempt.min(4));
                thread::sleep(Duration::from_millis(delay));
            }
            Err(error) => return Err(classify_file_io(error, from, operation)),
        }
    }
    unreachable!()
}

fn remove_file_if_exists_with_retry(path: &Path) -> Result<(), JobError> {
    let lp = long_path(path);
    for attempt in 0..FILE_IO_RETRIES {
        match fs::remove_file(&lp) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) => {
                if attempt + 1 == FILE_IO_RETRIES {
                    return Err(JobError::FileLocked(path.display().to_string()));
                }
                let delay = 100_u64.saturating_mul(1_u64 << attempt.min(4));
                thread::sleep(Duration::from_millis(delay));
            }
            Err(error) => return Err(classify_file_io(error, path, "remove transaction file")),
        }
    }
    unreachable!()
}

fn recovery_path(path: &Path) -> PathBuf {
    path.with_extension("previous")
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), JobError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("next");
    let recovery = recovery_path(path);
    let data = serde_json::to_vec(value)?;
    {
        let mut file = File::create(&temporary)?;
        file.write_all(&data)?;
        file.sync_all()?;
    }

    remove_file_if_exists(&recovery)?;
    if path.exists() {
        fs::rename(path, &recovery)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if recovery.exists() {
            let _ = fs::rename(&recovery, path);
        }
        return Err(error.into());
    }
    // The temporary file was already flushed, but reopen and flush the final
    // name before dropping the recovery copy. This keeps transaction intent
    // durable at the path recovery scans after a power loss.
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()
        .map_err(|error| classify_file_io(error, path, "flush committed transaction state"))?;
    remove_file_if_exists(&recovery)?;
    Ok(())
}

fn read_json_recovering<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, JobError> {
    if path.exists() {
        let bytes = fs::read(path)?;
        return Ok(Some(serde_json::from_slice(&bytes)?));
    }
    let recovery = recovery_path(path);
    if recovery.exists() {
        let bytes = fs::read(&recovery)?;
        return Ok(Some(serde_json::from_slice(&bytes)?));
    }
    Ok(None)
}

fn remove_file_if_exists(path: &Path) -> Result<(), JobError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_flat_files(directory: &Path) -> Result<(), JobError> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            remove_file_if_exists_with_retry(&entry.path())?;
        }
    }
    Ok(())
}

fn remove_empty_dir(directory: &Path) -> Result<(), JobError> {
    match fs::remove_dir(directory) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(offset: u64, data: &[u8]) -> ChunkRef {
        ChunkRef {
            hash: blake3::hash(data).to_hex().to_string(),
            file_offset: offset,
            uncompressed_size: data.len() as u64,
            pack_id: "pack-00000".to_string(),
            pack_offset: offset,
            compressed_size: data.len() as u64,
            compressed_sha256: sha256_bytes(data),
            codec: ChunkCodec::Raw,
            encryption: None,
        }
    }

    fn journal(install: &Path) -> JobJournal {
        default_journal(
            "game-with-a-very-long-identifier",
            "update",
            install.display().to_string(),
            "v1",
            "v2",
            0,
        )
    }

    #[test]
    fn allocation_hint_never_changes_logical_eof() {
        let root = env::temp_dir().join(format!(
            "0xolemon-allocation-eof-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("allocation.stage");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let outcome = try_reserve_file_allocation(&file, &path, 1024 * 1024, 1);
        assert_eq!(file.metadata().unwrap().len(), 0);
        assert!(outcome.reserved_bytes > 0 || outcome.fallback_reason.is_some());
        drop(file);
        fs::remove_file(&path).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn session_paths_are_short_and_stage_is_not_preallocated() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let first = b"first";
        let second = b"second";
        let file = FileEntry {
            path: "Very/Deep/Directory/With/Unicode-đ/File.pak".to_string(),
            size: (first.len() + second.len()) as u64,
            sha256: sha256_bytes(&[first.as_slice(), second.as_slice()].concat()),
            chunks: vec![chunk(0, first), chunk(first.len() as u64, second)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let mut session = SequentialUpdateSession::prepare(
            &downloading,
            &journal(&install),
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();
        let mut writer = session.open_writer(&file).unwrap();
        assert_eq!(writer.file.metadata().unwrap().len(), 0);
        assert!(writer.path.to_string_lossy().len() < 180);
        writer.append(&file.chunks[0], first).unwrap();
        session.checkpoint_writer(&mut writer, false).unwrap();
        assert_eq!(writer.checkpointed_bytes(), first.len() as u64);
        assert_eq!(writer.file.metadata().unwrap().len(), first.len() as u64);

        drop(writer);
        remove_flat_files(&session.stage_root).unwrap();
        remove_flat_files(&session.cache_root).unwrap();
        session.cleanup_session_files().unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn resume_truncates_bytes_after_the_durable_checkpoint() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-resume-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let first = b"first";
        let second = b"second";
        let file = FileEntry {
            path: "game.bin".to_string(),
            size: (first.len() + second.len()) as u64,
            sha256: sha256_bytes(&[first.as_slice(), second.as_slice()].concat()),
            chunks: vec![chunk(0, first), chunk(first.len() as u64, second)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        let mut session = SequentialUpdateSession::prepare(
            &downloading,
            &job,
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();
        let mut writer = session.open_writer(&file).unwrap();
        writer.append(&file.chunks[0], first).unwrap();
        session.checkpoint_writer(&mut writer, false).unwrap();
        writer.file.write_all(second).unwrap();
        drop(writer);

        let mut resumed =
            SequentialUpdateSession::open_owned_for_recovery(&downloading, &job, &install)
                .unwrap()
                .unwrap();
        let mut resumed_writer = resumed.open_writer(&file).unwrap();
        assert_eq!(resumed_writer.next_chunk(), 1);
        assert_eq!(
            resumed_writer.file.metadata().unwrap().len(),
            first.len() as u64
        );
        resumed_writer.append(&file.chunks[1], second).unwrap();
        resumed.finish_writer(&mut resumed_writer, &file).unwrap();

        drop(resumed_writer);
        remove_flat_files(&resumed.stage_root).unwrap();
        remove_flat_files(&resumed.cache_root).unwrap();
        resumed.cleanup_session_files().unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn large_target_does_not_allocate_its_logical_size_during_prepare() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-large-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let target_size = 15_200_000_000_u64;
        let file = FileEntry {
            path: "Content/Paks/large.ucas".to_string(),
            size: target_size,
            sha256: "0".repeat(64),
            chunks: vec![chunk(0, b"verified-prefix")],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        let mut session = SequentialUpdateSession::prepare(
            &downloading,
            &job,
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();
        let writer = session.open_writer(&file).unwrap();
        assert_eq!(writer.file.metadata().unwrap().len(), 0);
        assert_ne!(writer.file.metadata().unwrap().len(), target_size);
        drop(writer);
        session.cleanup_session_files().unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn expanded_delta_file_is_verified_and_committed_at_target_size() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-expanded-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();

        let base = b"existing-pak-prefix";
        let delta = b"-new-delta-tail";
        let target_bytes = [base.as_slice(), delta.as_slice()].concat();
        let target_path = install.join("Content").join("Paks").join("game.pak");
        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        fs::write(&target_path, base).unwrap();

        let file = FileEntry {
            path: "Content/Paks/game.pak".to_string(),
            size: target_bytes.len() as u64,
            sha256: sha256_bytes(&target_bytes),
            chunks: vec![chunk(0, base), chunk(base.len() as u64, delta)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        let mut session = SequentialUpdateSession::prepare(
            &downloading,
            &job,
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();

        let mut writer = session.open_writer(&file).unwrap();
        writer.append(&file.chunks[0], base).unwrap();
        session.checkpoint_writer(&mut writer, false).unwrap();
        writer.append(&file.chunks[1], delta).unwrap();
        session.finish_writer(&mut writer, &file).unwrap();
        drop(writer);
        session.commit_file(&install, &file).unwrap();

        assert_eq!(fs::metadata(&target_path).unwrap().len(), file.size);
        assert_eq!(fs::read(&target_path).unwrap(), target_bytes);

        write_install_marker_file(
            &install,
            &InstallMarker {
                game_id: job.game_id.clone(),
                version: "v2".to_string(),
                installed_at: Utc::now().to_rfc3339(),
                launch_executable: None,
                applied_patch_id: None,
            },
        )
        .unwrap();
        session.mark_marker_committed().unwrap();
        session.cleanup_committed(&install).unwrap();
        session.cleanup_session_files().unwrap();

        fs::remove_file(install_marker_path(&install)).unwrap();
        fs::remove_dir(install.join(INSTALL_MARKER_DIR)).unwrap();
        fs::remove_file(&target_path).unwrap();
        fs::remove_dir(target_path.parent().unwrap()).unwrap();
        fs::remove_dir(install.join("Content")).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn dropping_uncommitted_transaction_restores_the_base_file() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-rollback-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let base = b"base-version";
        let target = b"target-version-expanded";
        let target_path = install.join("game.pak");
        fs::write(&target_path, base).unwrap();
        let file = FileEntry {
            path: "game.pak".to_string(),
            size: target.len() as u64,
            sha256: sha256_bytes(target),
            chunks: vec![chunk(0, target)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        {
            let mut session = SequentialUpdateSession::prepare(
                &downloading,
                &job,
                "v1",
                "v2",
                &install,
                std::slice::from_ref(&file),
            )
            .unwrap();
            let mut writer = session.open_writer(&file).unwrap();
            writer.append(&file.chunks[0], target).unwrap();
            session.finish_writer(&mut writer, &file).unwrap();
            drop(writer);
            session.commit_file(&install, &file).unwrap();
            assert_eq!(fs::read(&target_path).unwrap(), target);
            assert!(
                SequentialUpdateSession::has_pending_transaction(&downloading, &job.id).unwrap()
            );
        }
        assert_eq!(fs::read(&target_path).unwrap(), base);
        assert!(!SequentialUpdateSession::has_pending_transaction(&downloading, &job.id).unwrap());

        SequentialUpdateSession::cleanup_owned_session(&downloading, &job.id).unwrap();
        fs::remove_file(&target_path).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn batch_intent_is_durable_before_the_first_file_rename() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-batch-intent-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();

        let first_base = b"first-base";
        let second_base = b"second-base";
        let first_target = b"first-target";
        let second_target = b"second-target";
        fs::write(install.join("first.bin"), first_base).unwrap();
        fs::write(install.join("second.bin"), second_base).unwrap();
        let files = vec![
            FileEntry {
                path: "first.bin".to_string(),
                size: first_target.len() as u64,
                sha256: sha256_bytes(first_target),
                chunks: vec![chunk(0, first_target)],
                executable: false,
                delta_patches: None,
                preserve: false,
            },
            FileEntry {
                path: "second.bin".to_string(),
                size: second_target.len() as u64,
                sha256: sha256_bytes(second_target),
                chunks: vec![chunk(0, second_target)],
                executable: false,
                delta_patches: None,
                preserve: false,
            },
        ];
        let job = journal(&install);
        {
            let mut session =
                SequentialUpdateSession::prepare(&downloading, &job, "v1", "v2", &install, &files)
                    .unwrap();
            for (file, bytes) in files.iter().zip([&first_target[..], &second_target[..]]) {
                let mut writer = session.open_writer(file).unwrap();
                writer.append(&file.chunks[0], bytes).unwrap();
                session.finish_writer(&mut writer, file).unwrap();
            }

            session.sync_staged_files(&files).unwrap();
            session.prepare_commit_batch(&install, &files).unwrap();
            let transaction = read_json_recovering::<UpdateTransaction>(&session.transaction_path)
                .unwrap()
                .unwrap();
            assert_eq!(transaction.files.len(), 2);
            assert!(transaction
                .files
                .iter()
                .all(|record| record.phase == TransactionFilePhase::Prepared));

            session.commit_file(&install, &files[0]).unwrap();
            assert_eq!(fs::read(install.join("first.bin")).unwrap(), first_target);
            assert_eq!(fs::read(install.join("second.bin")).unwrap(), second_base);
        }

        assert_eq!(fs::read(install.join("first.bin")).unwrap(), first_base);
        assert_eq!(fs::read(install.join("second.bin")).unwrap(), second_base);
        SequentialUpdateSession::cleanup_owned_session(&downloading, &job.id).unwrap();
        fs::remove_file(install.join("first.bin")).unwrap();
        fs::remove_file(install.join("second.bin")).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn repair_commit_proof_does_not_trust_an_unchanged_version_marker() {
        let root = env::temp_dir().join(format!(
            "0xolemon-repair-proof-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let base = b"known-good-base";
        let repaired = b"repaired-target";
        let target_path = install.join("game.bin");
        fs::write(&target_path, base).unwrap();
        let file = FileEntry {
            path: "game.bin".to_string(),
            size: repaired.len() as u64,
            sha256: sha256_bytes(repaired),
            chunks: vec![chunk(0, repaired)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let mut job = journal(&install);
        job.kind = "repair".to_string();
        job.from_version = "v2".to_string();
        write_install_marker_file(
            &install,
            &InstallMarker {
                game_id: job.game_id.clone(),
                version: "v2".to_string(),
                installed_at: Utc::now().to_rfc3339(),
                launch_executable: None,
                applied_patch_id: None,
            },
        )
        .unwrap();
        let original_marker = fs::read(install_marker_path(&install)).unwrap();
        {
            let mut session = SequentialUpdateSession::prepare_with_commit_proof(
                &downloading,
                &job,
                "v2",
                "v2",
                &install,
                std::slice::from_ref(&file),
                TransactionCommitProof::TransactionPhase,
            )
            .unwrap();
            let mut writer = session.open_writer(&file).unwrap();
            writer.append(&file.chunks[0], repaired).unwrap();
            session.finish_writer(&mut writer, &file).unwrap();
            drop(writer);
            session.commit_file(&install, &file).unwrap();
            session
                .backup_metadata_file(
                    &install,
                    &format!("{INSTALL_MARKER_DIR}/{INSTALL_MARKER_FILE}"),
                )
                .unwrap();
            write_install_marker_file(
                &install,
                &InstallMarker {
                    game_id: job.game_id.clone(),
                    version: "v2".to_string(),
                    installed_at: "replacement-marker".to_string(),
                    launch_executable: None,
                    applied_patch_id: Some("temporary".to_string()),
                },
            )
            .unwrap();
            assert_eq!(fs::read(&target_path).unwrap(), repaired);
        }
        assert_eq!(fs::read(&target_path).unwrap(), base);
        assert_eq!(
            fs::read(install_marker_path(&install)).unwrap(),
            original_marker
        );
        SequentialUpdateSession::cleanup_owned_session(&downloading, &job.id).unwrap();
        fs::remove_file(&target_path).unwrap();
        fs::remove_file(install_marker_path(&install)).unwrap();
        fs::remove_dir(install.join(INSTALL_MARKER_DIR)).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn repair_commit_proof_failure_keeps_rollback_armed() {
        let root = env::temp_dir().join(format!(
            "0xolemon-repair-proof-failure-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let base = b"base";
        let repaired = b"repaired";
        let target_path = install.join("game.bin");
        fs::write(&target_path, base).unwrap();
        let file = FileEntry {
            path: "game.bin".to_string(),
            size: repaired.len() as u64,
            sha256: sha256_bytes(repaired),
            chunks: vec![chunk(0, repaired)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let mut job = journal(&install);
        job.kind = "repair".to_string();
        {
            let mut session = SequentialUpdateSession::prepare_with_commit_proof(
                &downloading,
                &job,
                "v2",
                "v2",
                &install,
                std::slice::from_ref(&file),
                TransactionCommitProof::TransactionPhase,
            )
            .unwrap();
            let mut writer = session.open_writer(&file).unwrap();
            writer.append(&file.chunks[0], repaired).unwrap();
            session.finish_writer(&mut writer, &file).unwrap();
            drop(writer);
            session.commit_file(&install, &file).unwrap();

            let blocker = root.join("not-a-directory");
            fs::write(&blocker, b"block").unwrap();
            let transaction_path = session.transaction_path.clone();
            session.transaction_path = blocker.join(TRANSACTION_FILE);
            assert!(session.mark_marker_committed().is_err());
            assert!(session.rollback_armed);
            session.transaction_path = transaction_path;
        }
        assert_eq!(fs::read(&target_path).unwrap(), base);

        SequentialUpdateSession::cleanup_owned_session(&downloading, &job.id).unwrap();
        fs::remove_file(root.join("not-a-directory")).unwrap();
        fs::remove_file(&target_path).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn patch_recovery_accepts_an_update_journal_version_transition() {
        let root = env::temp_dir().join(format!(
            "0xolemon-update-patch-recovery-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let data = b"patch";
        let file = FileEntry {
            path: "patch.dll".to_string(),
            size: data.len() as u64,
            sha256: sha256_bytes(data),
            chunks: vec![chunk(0, data)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        let session = SequentialUpdateSession::prepare_with_commit_proof(
            &downloading,
            &job,
            "v2",
            "v2",
            &install,
            std::slice::from_ref(&file),
            TransactionCommitProof::AppliedPatchId("patch-v2".to_string()),
        )
        .unwrap();
        drop(session);

        let recovered =
            SequentialUpdateSession::open_owned_for_recovery(&downloading, &job, &install)
                .unwrap()
                .expect("patch session should belong to the update journal");
        recovered.cleanup_session_files().unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn recovery_skips_a_completed_update_session_for_its_pending_patch() {
        let root = env::temp_dir().join(format!(
            "0xolemon-update-patch-collision-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let base = b"base";
        let updated = b"updated";
        let update_path = install.join("game.bin");
        fs::write(&update_path, base).unwrap();
        let update_file = FileEntry {
            path: "game.bin".to_string(),
            size: updated.len() as u64,
            sha256: sha256_bytes(updated),
            chunks: vec![chunk(0, updated)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let patch_file = FileEntry {
            path: "patch.dll".to_string(),
            size: 5,
            sha256: sha256_bytes(b"patch"),
            chunks: vec![chunk(0, b"patch")],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        let mut completed_update = SequentialUpdateSession::prepare(
            &downloading,
            &job,
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&update_file),
        )
        .unwrap();
        let mut writer = completed_update.open_writer(&update_file).unwrap();
        writer.append(&update_file.chunks[0], updated).unwrap();
        completed_update
            .finish_writer(&mut writer, &update_file)
            .unwrap();
        drop(writer);
        completed_update
            .commit_file(&install, &update_file)
            .unwrap();
        write_install_marker_file(
            &install,
            &InstallMarker {
                game_id: job.game_id.clone(),
                version: "v2".to_string(),
                installed_at: Utc::now().to_rfc3339(),
                launch_executable: None,
                applied_patch_id: None,
            },
        )
        .unwrap();
        completed_update.mark_marker_committed().unwrap();
        completed_update.cleanup_committed(&install).unwrap();

        let mut pending_patch = SequentialUpdateSession::prepare_with_commit_proof(
            &downloading,
            &job,
            "v2",
            "v2",
            &install,
            std::slice::from_ref(&patch_file),
            TransactionCommitProof::AppliedPatchId("patch-v2".to_string()),
        )
        .unwrap();
        pending_patch
            .prepare_commit_batch(&install, std::slice::from_ref(&patch_file))
            .unwrap();
        assert_ne!(completed_update.job_key, pending_patch.job_key);

        let mut recovered =
            SequentialUpdateSession::open_owned_for_recovery(&downloading, &job, &install)
                .unwrap()
                .expect("pending patch transaction should be selected");
        assert_eq!(recovered.job_key, pending_patch.job_key);
        assert_eq!(
            recovered.transaction.commit_proof,
            TransactionCommitProof::AppliedPatchId("patch-v2".to_string())
        );
        recovered.rollback(&install).unwrap();
        recovered.cleanup_session_files().unwrap();
        pending_patch.rollback_armed = false;
        completed_update.cleanup_session_files().unwrap();

        fs::remove_file(&update_path).unwrap();
        fs::remove_file(install_marker_path(&install)).unwrap();
        fs::remove_dir(install.join(INSTALL_MARKER_DIR)).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn patch_file_and_metadata_roll_back_together_before_marker_commit() {
        let root = env::temp_dir().join(format!(
            "0xolemon-patch-proof-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        let marker_dir = install.join(INSTALL_MARKER_DIR);
        fs::create_dir_all(&marker_dir).unwrap();
        let base = b"old-patched-file";
        let patched = b"new-patched-file";
        let target_path = install.join("game.dll");
        fs::write(&target_path, base).unwrap();
        let old_patch_manifest = b"old-patch-metadata";
        let patch_manifest_path = applied_patch_manifest_path(&install);
        fs::write(&patch_manifest_path, old_patch_manifest).unwrap();
        let file = FileEntry {
            path: "game.dll".to_string(),
            size: patched.len() as u64,
            sha256: sha256_bytes(patched),
            chunks: vec![chunk(0, patched)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let mut job = journal(&install);
        job.kind = "patch".to_string();
        let old_patch_id = "patch-old".to_string();
        let new_patch_id = "patch-new".to_string();
        write_install_marker_file(
            &install,
            &InstallMarker {
                game_id: job.game_id.clone(),
                version: "v2".to_string(),
                installed_at: Utc::now().to_rfc3339(),
                launch_executable: None,
                applied_patch_id: Some(old_patch_id.clone()),
            },
        )
        .unwrap();
        {
            let mut session = SequentialUpdateSession::prepare_with_commit_proof(
                &downloading,
                &job,
                "v2",
                "v2",
                &install,
                std::slice::from_ref(&file),
                TransactionCommitProof::AppliedPatchId(new_patch_id),
            )
            .unwrap();
            let mut writer = session.open_writer(&file).unwrap();
            writer.append(&file.chunks[0], patched).unwrap();
            session.finish_writer(&mut writer, &file).unwrap();
            drop(writer);
            session.commit_file(&install, &file).unwrap();
            session
                .backup_metadata_file(
                    &install,
                    &format!("{INSTALL_MARKER_DIR}/{APPLIED_PATCH_MANIFEST_FILE}"),
                )
                .unwrap();
            session
                .backup_metadata_file(
                    &install,
                    &format!("{INSTALL_MARKER_DIR}/{INSTALL_MARKER_FILE}"),
                )
                .unwrap();
            fs::write(&patch_manifest_path, b"new-patch-metadata").unwrap();
            assert_eq!(fs::read(&target_path).unwrap(), patched);
        }
        assert_eq!(fs::read(&target_path).unwrap(), base);
        assert_eq!(fs::read(&patch_manifest_path).unwrap(), old_patch_manifest);
        assert_eq!(
            read_install_marker(&install)
                .unwrap()
                .unwrap()
                .applied_patch_id,
            Some(old_patch_id)
        );
        SequentialUpdateSession::cleanup_owned_session(&downloading, &job.id).unwrap();
        fs::remove_file(&target_path).unwrap();
        fs::remove_file(&patch_manifest_path).unwrap();
        fs::remove_file(install_marker_path(&install)).unwrap();
        fs::remove_dir(&marker_dir).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn rollback_removes_new_target_when_crash_precedes_installed_phase_checkpoint() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-new-file-crash-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let target = b"new-target-file";
        let target_path = install.join("new.bin");
        let file = FileEntry {
            path: "new.bin".to_string(),
            size: target.len() as u64,
            sha256: sha256_bytes(target),
            chunks: vec![chunk(0, target)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        {
            let mut session = SequentialUpdateSession::prepare(
                &downloading,
                &job,
                "v1",
                "v2",
                &install,
                std::slice::from_ref(&file),
            )
            .unwrap();
            let mut writer = session.open_writer(&file).unwrap();
            writer.append(&file.chunks[0], target).unwrap();
            session.finish_writer(&mut writer, &file).unwrap();
            drop(writer);

            session.transaction.files.push(TransactionFile {
                relative_path: file.path.clone(),
                had_original: false,
                obsolete: false,
                metadata: false,
                phase: TransactionFilePhase::Prepared,
            });
            session.persist_transaction().unwrap();
            let stage = session.stage_path_for_hash(&manifest_path_hash(&file.path));
            fs::rename(long_path(&stage), long_path(&target_path)).unwrap();
            assert!(target_path.is_file());
        }

        assert!(!target_path.exists());
        assert!(!SequentialUpdateSession::has_pending_transaction(&downloading, &job.id).unwrap());
        SequentialUpdateSession::cleanup_owned_session(&downloading, &job.id).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn committed_marker_prevents_drop_rollback_and_recovery_cleans_backup() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-marker-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let base = b"base-version";
        let target = b"target-version-expanded";
        let target_path = install.join("game.pak");
        fs::write(&target_path, base).unwrap();
        let file = FileEntry {
            path: "game.pak".to_string(),
            size: target.len() as u64,
            sha256: sha256_bytes(target),
            chunks: vec![chunk(0, target)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let job = journal(&install);
        {
            let mut session = SequentialUpdateSession::prepare(
                &downloading,
                &job,
                "v1",
                "v2",
                &install,
                std::slice::from_ref(&file),
            )
            .unwrap();
            let mut writer = session.open_writer(&file).unwrap();
            writer.append(&file.chunks[0], target).unwrap();
            session.finish_writer(&mut writer, &file).unwrap();
            drop(writer);
            session.commit_file(&install, &file).unwrap();
            write_install_marker_file(
                &install,
                &InstallMarker {
                    game_id: job.game_id.clone(),
                    version: "v2".to_string(),
                    installed_at: Utc::now().to_rfc3339(),
                    launch_executable: None,
                    applied_patch_id: None,
                },
            )
            .unwrap();
        }
        assert_eq!(fs::read(&target_path).unwrap(), target);

        let mut resumed =
            SequentialUpdateSession::open_owned_for_recovery(&downloading, &job, &install)
                .unwrap()
                .unwrap();
        assert_eq!(
            resumed.recover(&install, "v2").unwrap(),
            RecoveryOutcome::AlreadyCommitted
        );
        assert!(!SequentialUpdateSession::has_pending_transaction(&downloading, &job.id).unwrap());
        resumed.cleanup_session_files().unwrap();

        fs::remove_file(install_marker_path(&install)).unwrap();
        fs::remove_dir(install.join(INSTALL_MARKER_DIR)).unwrap();
        fs::remove_file(&target_path).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn missing_stage_resets_durable_checkpoint_without_preallocation() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-stage-missing-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let data = b"checkpointed";
        let file = FileEntry {
            path: "game.bin".to_string(),
            size: data.len() as u64,
            sha256: sha256_bytes(data),
            chunks: vec![chunk(0, data)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let mut session = SequentialUpdateSession::prepare(
            &downloading,
            &journal(&install),
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();
        let mut writer = session.open_writer(&file).unwrap();
        writer.append(&file.chunks[0], data).unwrap();
        session.checkpoint_writer(&mut writer, false).unwrap();
        let stage_path = writer.path.clone();
        drop(writer);
        fs::remove_file(&stage_path).unwrap();

        let repaired = session.open_writer(&file).unwrap();
        assert_eq!(repaired.next_chunk(), 0);
        assert_eq!(repaired.file.metadata().unwrap().len(), 0);
        drop(repaired);
        session.cleanup_session_files().unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn shared_cached_chunk_is_removed_only_after_every_destination_checkpoint() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-shared-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let data = b"shared-data";
        let shared = chunk(0, data);
        let files = vec![
            FileEntry {
                path: "one.bin".to_string(),
                size: data.len() as u64,
                sha256: sha256_bytes(data),
                chunks: vec![shared.clone()],
                executable: false,
                delta_patches: None,
                preserve: false,
            },
            FileEntry {
                path: "two.bin".to_string(),
                size: data.len() as u64,
                sha256: sha256_bytes(data),
                chunks: vec![shared.clone()],
                executable: false,
                delta_patches: None,
                preserve: false,
            },
        ];
        let mut session = SequentialUpdateSession::prepare(
            &downloading,
            &journal(&install),
            "v1",
            "v2",
            &install,
            &files,
        )
        .unwrap();
        let cached = staged_chunk_path_from(session.cache_root(), &shared.hash);
        fs::write(&cached, data).unwrap();

        session
            .release_checkpointed_chunks(&mut vec![shared.hash.clone()])
            .unwrap();
        assert!(cached.exists());
        session
            .release_checkpointed_chunks(&mut vec![shared.hash.clone()])
            .unwrap();
        assert!(!cached.exists());

        session.cleanup_session_files().unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn full_file_hash_mismatch_never_reaches_the_install_target() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-hash-mismatch-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let data = b"verified-chunk-with-wrong-file-hash";
        let file = FileEntry {
            path: "large.pak".to_string(),
            size: data.len() as u64,
            sha256: "0".repeat(64),
            chunks: vec![chunk(0, data)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let mut session = SequentialUpdateSession::prepare(
            &downloading,
            &journal(&install),
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();
        let mut writer = session.open_writer(&file).unwrap();
        writer.append(&file.chunks[0], data).unwrap();
        let error = session.finish_writer(&mut writer, &file).unwrap_err();
        assert!(matches!(
            error,
            JobError::Depot(message) if message.contains("staging hash mismatch")
        ));
        assert!(!install.join(&file.path).exists());

        drop(writer);
        session.cleanup_session_files().unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn committed_recovery_keeps_metadata_backup_when_replacement_is_missing() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-metadata-recovery-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(install.join(INSTALL_MARKER_DIR)).unwrap();
        let original = b"damaged";
        let repaired = b"repaired";
        let target_path = install.join("game.bin");
        fs::write(&target_path, original).unwrap();
        let file = FileEntry {
            path: "game.bin".to_string(),
            size: repaired.len() as u64,
            sha256: sha256_bytes(repaired),
            chunks: vec![chunk(0, repaired)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let mut job = journal(&install);
        job.kind = "repair".to_string();
        job.from_version = "v2".to_string();
        let marker_path = install_marker_path(&install);
        write_install_marker_file(
            &install,
            &InstallMarker {
                game_id: job.game_id.clone(),
                version: "v2".to_string(),
                installed_at: "original-marker".to_string(),
                launch_executable: None,
                applied_patch_id: None,
            },
        )
        .unwrap();

        let backup_path;
        {
            let mut session = SequentialUpdateSession::prepare_with_commit_proof(
                &downloading,
                &job,
                "v2",
                "v2",
                &install,
                std::slice::from_ref(&file),
                TransactionCommitProof::TransactionPhase,
            )
            .unwrap();
            let mut writer = session.open_writer(&file).unwrap();
            writer.append(&file.chunks[0], repaired).unwrap();
            session.finish_writer(&mut writer, &file).unwrap();
            drop(writer);
            session.commit_file(&install, &file).unwrap();
            session
                .backup_metadata_file(
                    &install,
                    &format!("{INSTALL_MARKER_DIR}/{INSTALL_MARKER_FILE}"),
                )
                .unwrap();
            backup_path = transaction_backup_path(&marker_path, &session.job_key).unwrap();
            write_install_marker_file(
                &install,
                &InstallMarker {
                    game_id: job.game_id.clone(),
                    version: "v2".to_string(),
                    installed_at: "repaired-marker".to_string(),
                    launch_executable: None,
                    applied_patch_id: None,
                },
            )
            .unwrap();
            session.mark_marker_committed().unwrap();
        }

        fs::remove_file(&marker_path).unwrap();
        let mut recovered =
            SequentialUpdateSession::open_owned_for_recovery(&downloading, &job, &install)
                .unwrap()
                .unwrap();
        let error = recovered.recover(&install, "v2").unwrap_err();
        assert!(matches!(
            error,
            JobError::Depot(message) if message.contains("transaction metadata is missing")
        ));
        assert!(backup_path.is_file());
        assert_eq!(fs::read(&target_path).unwrap(), repaired);

        // Restore the pre-transaction state so the test leaves no owned files.
        recovered.transaction.phase = TransactionPhase::FilesInstalled;
        recovered.rollback_armed = true;
        recovered.rollback(&install).unwrap();
        recovered.cleanup_session_files().unwrap();
        assert_eq!(fs::read(&target_path).unwrap(), original);
        assert!(marker_path.is_file());
        fs::remove_file(&target_path).unwrap();
        fs::remove_file(&marker_path).unwrap();
        fs::remove_dir(install.join(INSTALL_MARKER_DIR)).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn old_job_cleanup_does_not_touch_a_new_job_session() {
        let root = env::temp_dir().join(format!(
            "0xolemon-sequential-ownership-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("dl").join("123456");
        let install = root.join("common").join("Game");
        fs::create_dir_all(&install).unwrap();
        let data = b"target";
        let file = FileEntry {
            path: "game.bin".to_string(),
            size: data.len() as u64,
            sha256: sha256_bytes(data),
            chunks: vec![chunk(0, data)],
            executable: false,
            delta_patches: None,
            preserve: false,
        };
        let first_job = journal(&install);
        let mut second_job = journal(&install);
        second_job.id = format!("{}-replacement", first_job.id);
        let first = SequentialUpdateSession::prepare(
            &downloading,
            &first_job,
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();
        let second = SequentialUpdateSession::prepare(
            &downloading,
            &second_job,
            "v1",
            "v2",
            &install,
            std::slice::from_ref(&file),
        )
        .unwrap();
        let second_state = second.state_path.clone();

        SequentialUpdateSession::cleanup_owned_session(&downloading, &first_job.id).unwrap();
        assert!(second_state.exists());
        drop(first);
        drop(second);
        SequentialUpdateSession::cleanup_owned_session(&downloading, &second_job.id).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(install.parent().unwrap()).unwrap();
        fs::remove_dir(downloading.parent().unwrap()).unwrap();
        fs::remove_dir(&root).unwrap();
    }
}
