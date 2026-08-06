use super::*;

const SEQUENTIAL_STAGE_SCHEMA: u32 = 3;
const SESSION_DIRECTORY: &str = "s";
const STAGE_DIRECTORY: &str = "f";
const CACHE_DIRECTORY: &str = "c";
const STATE_FILE: &str = "state.json";
const TRANSACTION_FILE: &str = "txn.json";
const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(2);
const FILE_IO_RETRIES: usize = 12;

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
    phase: TransactionFilePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateTransaction {
    schema_version: u32,
    job_id: String,
    target_version: String,
    phase: TransactionPhase,
    files: Vec<TransactionFile>,
}

pub(super) struct SequentialFileWriter {
    path_hash: String,
    path: PathBuf,
    file: File,
    next_chunk: usize,
    durable_bytes: u64,
    uncheckpointed_bytes: u64,
    last_checkpoint: Instant,
}

impl SequentialFileWriter {
    pub(super) fn next_chunk(&self) -> usize {
        self.next_chunk
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
        self.next_chunk = self.next_chunk.saturating_add(1);
        self.durable_bytes = self.durable_bytes.saturating_add(data.len() as u64);
        self.uncheckpointed_bytes = self.uncheckpointed_bytes.saturating_add(data.len() as u64);
        Ok(())
    }

    pub(super) fn checkpoint_due(&self) -> bool {
        self.uncheckpointed_bytes >= DOWNLOAD_CHECKPOINT_BYTES
            || self.last_checkpoint.elapsed() >= CHECKPOINT_INTERVAL
    }
}

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
            if state.schema_version != SEQUENTIAL_STAGE_SCHEMA
                || state.game_id != journal.game_id
                || state.install_path_fingerprint != expected_fingerprint
                || state.from_version != journal.from_version
                || state.target_version != journal.to_version
            {
                return Err(JobError::SessionMismatch(
                    "startup recovery ownership does not match the update journal".to_string(),
                ));
            }
            let transaction_path = session_root.join(TRANSACTION_FILE);
            let Some(transaction) = read_json_recovering::<UpdateTransaction>(&transaction_path)?
            else {
                return Ok(None);
            };
            if transaction.job_id != journal.id || transaction.target_version != journal.to_version
            {
                return Err(JobError::SessionMismatch(
                    "startup recovery transaction belongs to another update".to_string(),
                ));
            }
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
        };
        session.persist_state()?;
        session.persist_transaction()?;
        session.validate_path_budget(install_root, files)?;
        Ok(session)
    }

    pub(super) fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    pub(super) fn durable_chunks(&self, file: &FileEntry) -> usize {
        self.state
            .files
            .get(&manifest_path_hash(&file.path))
            .map(|state| state.durable_chunks.min(file.chunks.len()))
            .unwrap_or(0)
    }

    pub(super) fn remove_cached_chunk(&self, hash: &str) -> Result<(), JobError> {
        remove_file_if_exists_with_retry(&staged_chunk_path_from(&self.cache_root, hash))
    }

    pub(super) fn reconcile_chunk_references(
        &mut self,
        files: &[FileEntry],
    ) -> Result<(), JobError> {
        reconcile_shared_chunk_uses(&mut self.state, files);
        self.persist_state()
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
        self.persist_state()?;
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

        let marker_committed = read_install_marker(install_root)?
            .and_then(|marker| usable_installed_version(&marker.version))
            .is_some_and(|version| version == target_version);

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
        }

        handle.seek(SeekFrom::Start(durable_bytes))?;
        Ok(SequentialFileWriter {
            path_hash,
            path: stage_path,
            file: handle,
            next_chunk: durable_chunks,
            durable_bytes,
            uncheckpointed_bytes: 0,
            last_checkpoint: Instant::now(),
        })
    }

    pub(super) fn checkpoint_writer(
        &mut self,
        writer: &mut SequentialFileWriter,
        complete: bool,
    ) -> Result<(), JobError> {
        writer
            .file
            .sync_data()
            .map_err(|error| classify_file_io(error, &writer.path, "checkpoint staging file"))?;
        self.update_progress(
            &writer.path_hash,
            writer.next_chunk,
            writer.durable_bytes,
            complete,
        )?;
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
        self.checkpoint_writer(writer, false)?;
        writer
            .file
            .sync_all()
            .map_err(|error| classify_file_io(error, &writer.path, "flush staging file"))?;
        let actual = sha256_file(&long_path(&writer.path))?;
        if actual != file.sha256 {
            return Err(JobError::Depot(format!(
                "staging hash mismatch: {}",
                file.path
            )));
        }
        self.update_progress(
            &writer.path_hash,
            writer.next_chunk,
            writer.durable_bytes,
            true,
        )
    }

    pub(super) fn commit_file(
        &mut self,
        install_root: &Path,
        file: &FileEntry,
    ) -> Result<Option<(PathBuf, PathBuf)>, JobError> {
        let path_hash = manifest_path_hash(&file.path);
        let stage = self.stage_path_for_hash(&path_hash);
        let target = safe_join(install_root, &file.path)
            .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(&long_path(parent))?;
        }
        let backup = transaction_backup_path(&target, &self.job_key)?;
        let had_original = long_path(&target).exists();

        if long_path(&backup).exists() {
            return Err(JobError::SessionMismatch(format!(
                "unexpected transaction backup already exists for {}",
                file.path
            )));
        }

        self.transaction.files.push(TransactionFile {
            relative_path: file.path.clone(),
            had_original,
            obsolete: false,
            phase: TransactionFilePhase::Prepared,
        });
        self.persist_transaction()?;

        if had_original {
            rename_with_retry(&target, &backup, "back up installed file")?;
            self.set_transaction_file_phase(&file.path, TransactionFilePhase::BackedUp)?;
        }

        if let Err(error) = rename_with_retry(&stage, &target, "install verified staging file") {
            if had_original && long_path(&backup).exists() {
                let _ = rename_with_retry(&backup, &target, "restore installed file");
            }
            return Err(error);
        }
        self.set_transaction_file_phase(&file.path, TransactionFilePhase::Installed)?;
        self.transaction.phase = TransactionPhase::FilesInstalled;
        self.persist_transaction()?;

        Ok(had_original.then_some((target, backup)))
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
        let backup = transaction_backup_path(&target, &self.job_key)?;
        if long_path(&backup).exists() {
            return Err(JobError::SessionMismatch(format!(
                "unexpected obsolete backup already exists for {relative_path}"
            )));
        }
        self.transaction.files.push(TransactionFile {
            relative_path: relative_path.to_string(),
            had_original: true,
            obsolete: true,
            phase: TransactionFilePhase::Prepared,
        });
        self.persist_transaction()?;
        rename_with_retry(&target, &backup, "back up obsolete file")?;
        self.set_transaction_file_phase(relative_path, TransactionFilePhase::ObsoleteBackedUp)
    }

    pub(super) fn mark_marker_committed(&mut self) -> Result<(), JobError> {
        // write_install_marker has already committed the target at this point.
        // Disarm first so a txn.json write failure cannot roll files back while
        // leaving a target-version marker behind.
        self.rollback_armed = false;
        self.transaction.phase = TransactionPhase::MarkerCommitted;
        self.persist_transaction()?;
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
        self.persist_state()
    }

    fn set_transaction_file_phase(
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
        self.persist_transaction()
    }

    fn update_progress(
        &mut self,
        path_hash: &str,
        durable_chunks: usize,
        durable_bytes: u64,
        complete: bool,
    ) -> Result<(), JobError> {
        let state = self.state.files.get_mut(path_hash).ok_or_else(|| {
            JobError::Depot(format!("missing sequential progress for {path_hash}"))
        })?;
        state.durable_chunks = durable_chunks;
        state.durable_bytes = durable_bytes;
        state.complete = complete;
        self.persist_state()
    }

    fn persist_state(&self) -> Result<(), JobError> {
        write_json_atomic(&self.state_path, &self.state)
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
            let marker_is_target = read_install_marker(&self.install_root)
                .ok()
                .flatten()
                .and_then(|marker| usable_installed_version(&marker.version))
                .is_some_and(|version| version == self.transaction.target_version);
            if marker_is_target {
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
    let data = serde_json::to_vec_pretty(value)?;
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
        let resumed_writer = resumed.open_writer(&file).unwrap();
        assert_eq!(resumed_writer.next_chunk(), 1);
        assert_eq!(
            resumed_writer.file.metadata().unwrap().len(),
            first.len() as u64
        );

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
