use super::*;

const DIRECT_STAGE_SCHEMA: u32 = 1;
const DIRECT_STAGE_SUFFIX: &str = "007v2.stage";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct DirectStageState {
    schema_version: u32,
    plan_id: String,
    target_version: String,
    #[serde(default)]
    completed_hashes: HashSet<String>,
}

#[derive(Debug, Clone)]
struct ChunkDestination {
    path: PathBuf,
    offset: u64,
}

#[derive(Debug, Clone)]
pub(super) struct DirectStagePlan {
    staging_root: PathBuf,
    state_path: PathBuf,
    state: Arc<Mutex<DirectStageState>>,
    destinations: Arc<HashMap<String, Vec<ChunkDestination>>>,
}

impl DirectStagePlan {
    pub(super) fn prepare(
        downloading_root: &Path,
        staging_root: &Path,
        files: &[FileEntry],
        target_version: &str,
    ) -> Result<Self, JobError> {
        fs::create_dir_all(staging_root)?;
        let plan_id = direct_plan_id(files);
        let state_path = downloading_root.join("direct-stage-state.json");
        let mut state = load_direct_state(&state_path).unwrap_or_else(|| DirectStageState {
            schema_version: DIRECT_STAGE_SCHEMA,
            plan_id: plan_id.clone(),
            target_version: target_version.to_string(),
            completed_hashes: HashSet::new(),
        });
        if state.schema_version != DIRECT_STAGE_SCHEMA
            || state.plan_id != plan_id
            || state.target_version != target_version
        {
            state = DirectStageState {
                schema_version: DIRECT_STAGE_SCHEMA,
                plan_id,
                target_version: target_version.to_string(),
                completed_hashes: HashSet::new(),
            };
        }

        let mut destinations: HashMap<String, Vec<ChunkDestination>> = HashMap::new();
        for file in files {
            let stage_path = direct_stage_path(staging_root, file)?;
            let lp_stage = long_path(&stage_path);
            if let Some(parent) = lp_stage.parent() {
                fs::create_dir_all(parent)?;
            }
            let stage = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&lp_stage)
                .map_err(|e| JobError::Depot(format!("failed to open staging file '{}': {e}", stage_path.display())))?;
            if stage.metadata()?.len() != file.size {
                stage.set_len(file.size)?;
                stage.sync_all()?;
            }
            for chunk in &file.chunks {
                destinations
                    .entry(chunk.hash.clone())
                    .or_default()
                    .push(ChunkDestination {
                        path: stage_path.clone(),
                        offset: chunk.file_offset,
                    });
            }
        }

        let plan = Self {
            staging_root: staging_root.to_path_buf(),
            state_path,
            state: Arc::new(Mutex::new(state)),
            destinations: Arc::new(destinations),
        };
        plan.revalidate_completed(files)?;
        plan.persist_state()?;
        Ok(plan)
    }

    pub(super) fn filter_missing_chunks(
        &self,
        local_sources: &HashMap<String, LocalChunkSource>,
        files: &[FileEntry],
    ) -> Vec<ChunkRef> {
        let completed = self
            .state
            .lock()
            .map(|state| state.completed_hashes.clone())
            .unwrap_or_default();
        let mut seen = HashSet::new();
        files
            .iter()
            .flat_map(|file| file.chunks.iter())
            .filter(|chunk| seen.insert(chunk.hash.clone()))
            .filter(|chunk| !completed.contains(&chunk.hash))
            .filter(|chunk| !local_sources.contains_key(&chunk.hash))
            .cloned()
            .collect()
    }

    pub(super) fn write_local_chunks(
        &self,
        files: &[FileEntry],
        local_sources: &HashMap<String, LocalChunkSource>,
        control: &JobControl,
    ) -> Result<(), JobError> {
        let chunks = unique_chunks(files);
        let completed = self
            .state
            .lock()
            .map(|state| state.completed_hashes.clone())
            .unwrap_or_default();
        let mut pending_hashes = Vec::new();
        let mut pending_paths = HashSet::new();
        let mut pending_bytes = 0_u64;

        for chunk in chunks {
            if control.is_canceled() {
                return Err(JobError::Canceled);
            }
            if completed.contains(&chunk.hash) {
                continue;
            }
            let Some(source) = local_sources.get(&chunk.hash) else {
                continue;
            };
            let mut input = File::open(&source.path)?;
            input.seek(SeekFrom::Start(source.offset))?;
            let mut data = vec![0_u8; source.size as usize];
            input.read_exact(&mut data)?;
            verify_chunk_bytes(&chunk, &data)?;
            for path in self.write_decoded_chunk(&chunk, &data)? {
                pending_paths.insert(path);
            }
            pending_bytes = pending_bytes.saturating_add(data.len() as u64);
            pending_hashes.push(chunk.hash);
            if pending_bytes >= DOWNLOAD_CHECKPOINT_BYTES {
                self.checkpoint(&pending_paths, &pending_hashes)?;
                pending_paths.clear();
                pending_hashes.clear();
                pending_bytes = 0;
            }
        }
        self.checkpoint(&pending_paths, &pending_hashes)
    }

    pub(super) fn write_transport_chunks(
        &self,
        chunks: &[ChunkRef],
        range_start: u64,
        range: &[u8],
    ) -> Result<(), JobError> {
        let completed = self
            .state
            .lock()
            .map(|state| state.completed_hashes.clone())
            .unwrap_or_default();
        let mut paths = HashSet::new();
        let mut hashes = Vec::new();
        for chunk in chunks {
            if completed.contains(&chunk.hash) {
                continue;
            }
            let start = chunk.pack_offset.saturating_sub(range_start) as usize;
            let end = start.saturating_add(chunk.compressed_size as usize);
            if end > range.len() {
                return Err(JobError::Depot(format!(
                    "pack range does not contain chunk {}",
                    chunk.hash
                )));
            }
            let transport = &range[start..end];
            verify_compressed_chunk_bytes(chunk, transport)?;
            let encoded = decode_transport_chunk(chunk, transport)?;
            let data = decode_chunk_payload(chunk, &encoded)?;
            verify_chunk_bytes(chunk, &data)?;
            for path in self.write_decoded_chunk(chunk, &data)? {
                paths.insert(path);
            }
            hashes.push(chunk.hash.clone());
        }
        self.checkpoint(&paths, &hashes)
    }

    pub(super) fn commit_files(
        &self,
        install_root: &Path,
        files: &[FileEntry],
    ) -> Result<(), JobError> {
        let mut transactions = Vec::with_capacity(files.len());
        for file in files {
            let stage = direct_stage_path(&self.staging_root, file)?;
            let lp_stage = long_path(&stage);
            if !lp_stage.exists() || fs::metadata(&lp_stage)?.len() != file.size {
                return Err(JobError::Depot(format!(
                    "direct staging file is incomplete: {}",
                    file.path
                )));
            }
            let actual = sha256_file(&lp_stage)?;
            if actual != file.sha256 {
                return Err(JobError::Depot(format!(
                    "direct staging hash mismatch: {}",
                    file.path
                )));
            }
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&lp_stage)?
                .sync_all()?;
            let target = safe_join(install_root, &file.path)
                .ok_or_else(|| JobError::Depot(format!("unsafe manifest path: {}", file.path)))?;
            let lp_target = long_path(&target);
            if let Some(parent) = lp_target.parent() {
                fs::create_dir_all(parent)?;
            }
            let backup = sibling_path(&target, "007launcher.bak")?;
            let lp_backup = long_path(&backup);
            // Store logical (non-prefixed) paths in transactions for rollback tracking,
            // but use the long-path versions for actual I/O below.
            transactions.push((stage, lp_stage, target, lp_target, backup, lp_backup));
        }

        let mut committed: Vec<(PathBuf, PathBuf, PathBuf, bool)> = Vec::with_capacity(transactions.len());
        for (stage, lp_stage, target, lp_target, _backup, lp_backup) in transactions {
            if lp_backup.exists() {
                fs::remove_file(&lp_backup)?;
            }
            let had_original = lp_target.exists();
            if lp_target.exists() {
                fs::rename(&lp_target, &lp_backup)?;
            }
            if let Err(error) = fs::rename(&lp_stage, &lp_target) {
                if lp_backup.exists() {
                    let _ = fs::rename(&lp_backup, &lp_target);
                }
                rollback_direct_commits(&committed);
                return Err(JobError::Depot(format!(
                    "failed to commit '{}' to install: {error}",
                    stage.display()
                )));
            }
            committed.push((lp_target, lp_backup, target, had_original));
        }

        for (_, lp_backup, _, _) in &committed {
            if lp_backup.exists() {
                fs::remove_file(lp_backup)?;
            }
        }
        if self.state_path.exists() {
            fs::remove_file(&self.state_path)?;
        }
        Ok(())
    }

    fn write_decoded_chunk(&self, chunk: &ChunkRef, data: &[u8]) -> Result<Vec<PathBuf>, JobError> {
        let destinations = self.destinations.get(&chunk.hash).ok_or_else(|| {
            JobError::Depot(format!("no direct staging destination for {}", chunk.hash))
        })?;
        let mut touched = Vec::with_capacity(destinations.len());
        for destination in destinations {
            let lp = long_path(&destination.path);
            let mut output = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .open(&lp)
                .map_err(|e| JobError::Depot(format!("failed to write chunk to staging '{}': {e}", destination.path.display())))?;
            output.seek(SeekFrom::Start(destination.offset))?;
            output.write_all(data)?;
            touched.push(destination.path.clone());
        }
        Ok(touched)
    }

    fn checkpoint(&self, paths: &HashSet<PathBuf>, hashes: &[String]) -> Result<(), JobError> {
        if hashes.is_empty() {
            return Ok(());
        }
        for path in paths {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(long_path(path))?
                .sync_all()?;
        }
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| JobError::Depot("direct stage state lock poisoned".to_string()))?;
            state.completed_hashes.extend(hashes.iter().cloned());
        }
        self.persist_state()
    }

    fn persist_state(&self) -> Result<(), JobError> {
        let data = {
            let state = self
                .state
                .lock()
                .map_err(|_| JobError::Depot("direct stage state lock poisoned".to_string()))?;
            serde_json::to_vec_pretty(&*state)?
        };
        let temporary = self.state_path.with_extension("json.tmp");
        {
            let mut file = File::create(&temporary)?;
            file.write_all(&data)?;
            file.sync_all()?;
        }
        if self.state_path.exists() {
            fs::remove_file(&self.state_path)?;
        }
        fs::rename(temporary, &self.state_path)?;
        Ok(())
    }

    fn revalidate_completed(&self, files: &[FileEntry]) -> Result<(), JobError> {
        let chunks = unique_chunks(files)
            .into_iter()
            .map(|chunk| (chunk.hash.clone(), chunk))
            .collect::<HashMap<_, _>>();
        let completed = self
            .state
            .lock()
            .map(|state| state.completed_hashes.clone())
            .unwrap_or_default();
        let mut invalid = Vec::new();
        for hash in completed {
            let Some(chunk) = chunks.get(&hash) else {
                invalid.push(hash);
                continue;
            };
            let Some(destination) = self.destinations.get(&hash).and_then(|items| items.first())
            else {
                invalid.push(hash);
                continue;
            };
            let lp = long_path(&destination.path);
            let mut file = match File::open(&lp) {
                Ok(f) => f,
                Err(_) => { invalid.push(hash); continue; }
            };
            file.seek(SeekFrom::Start(destination.offset))?;
            let mut bytes = vec![0_u8; chunk.uncompressed_size as usize];
            if file.read_exact(&mut bytes).is_err() || verify_chunk_bytes(chunk, &bytes).is_err() {
                invalid.push(hash);
            }
        }
        if !invalid.is_empty() {
            let mut state = self
                .state
                .lock()
                .map_err(|_| JobError::Depot("direct stage state lock poisoned".to_string()))?;
            for hash in invalid {
                state.completed_hashes.remove(&hash);
            }
        }
        Ok(())
    }
}

fn rollback_direct_commits(committed: &[(PathBuf, PathBuf, PathBuf, bool)]) {
    // committed = (lp_target, lp_backup, _logical_target, had_original)
    for (lp_target, lp_backup, _, had_original) in committed.iter().rev() {
        if lp_target.exists() {
            let _ = fs::remove_file(lp_target);
        }
        if *had_original && lp_backup.exists() {
            let _ = fs::rename(lp_backup, lp_target);
        }
    }
}

impl DepotSource {
    pub(super) fn download_chunks_direct_to_staging<F>(
        &self,
        staged_chunks_root: &Path,
        stage: &DirectStagePlan,
        chunks: &[ChunkRef],
        control: Arc<JobControl>,
        mut on_progress: F,
    ) -> Result<(), JobError>
    where
        F: FnMut(DownloadProgress) -> Result<(), JobError>,
    {
        if chunks.is_empty() {
            return Ok(());
        }
        let tasks = build_pack_download_tasks(chunks, self.effective_pack_range_task_bytes());
        let settings = crate::platform::current_settings();
        let queue_budget = settings.download_queue_mb.saturating_mul(1024 * 1024);
        let workers_by_budget = (queue_budget / self.effective_pack_range_task_bytes()).max(1) as usize;
        let worker_count = self.effective_worker_count()
            .min(workers_by_budget)
            .min(tasks.len())
            .max(1);
        let tasks = Arc::new(Mutex::new(VecDeque::from(tasks)));
        let abort = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<Result<DownloadProgress, String>>();
        let mut first_error = None;

        thread::scope(|scope| {
            for _ in 0..worker_count {
                let tasks = Arc::clone(&tasks);
                let abort = Arc::clone(&abort);
                let tx = tx.clone();
                let source = self.clone();
                let control = Arc::clone(&control);
                let staged_chunks_root = staged_chunks_root.to_path_buf();
                let stage = stage.clone();
                scope.spawn(move || loop {
                    if abort.load(Ordering::SeqCst) || control.is_canceled() {
                        break;
                    }
                    while control.is_paused() {
                        if control.is_canceled() {
                            return;
                        }
                        thread::sleep(Duration::from_millis(150));
                    }
                    let Some(task) = tasks
                        .lock()
                        .expect("download task queue poisoned")
                        .pop_front()
                    else {
                        break;
                    };
                    let task_id = task.id();
                    let mut retry_count = 0_u32;
                    loop {
                        let result = source.download_pack_task_to_direct(
                            &staged_chunks_root,
                            &stage,
                            &task,
                            &task_id,
                            &control,
                            &tx,
                        );
                        match result {
                            Ok(()) => break,
                            Err(err) if retry_count < download_retry_count() => {
                                let next_retry = retry_count.saturating_add(1);
                                let Some(delay) = err.retry_delay(next_retry) else {
                                    abort.store(true, Ordering::SeqCst);
                                    let _ = tx.send(Err(err.to_string()));
                                    break;
                                };
                                retry_count = next_retry;
                                observe_adaptive_range(0, true);
                                let _ = tx.send(Ok(DownloadProgress {
                                    task_id: task_id.clone(),
                                    committed_bytes: 0,
                                    in_flight_bytes: 0,
                                    clear_in_flight: true,
                                    retry_count,
                                    rate_bytes_per_second: 0,
                                    retry_wait_ms: delay.as_millis().min(u128::from(u64::MAX))
                                        as u64,
                                    rate_limit_wait_ms: if matches!(
                                        err,
                                        JobError::RateLimited { .. }
                                    ) {
                                        delay.as_millis().min(u128::from(u64::MAX)) as u64
                                    } else {
                                        0
                                    },
                                }));
                                if let Err(error) = sleep_with_control(delay, &control) {
                                    abort.store(true, Ordering::SeqCst);
                                    let _ = tx.send(Err(error.to_string()));
                                    break;
                                }
                            }
                            Err(error) => {
                                abort.store(true, Ordering::SeqCst);
                                let _ = tx.send(Err(error.to_string()));
                                break;
                            }
                        }
                    }
                });
            }
            drop(tx);
            for message in rx {
                match message {
                    Ok(progress) => {
                        if let Err(error) = on_progress(progress) {
                            abort.store(true, Ordering::SeqCst);
                            first_error = Some(error.to_string());
                            break;
                        }
                    }
                    Err(error) => {
                        abort.store(true, Ordering::SeqCst);
                        first_error = Some(error);
                        break;
                    }
                }
            }
        });
        if let Some(error) = first_error {
            if error.eq_ignore_ascii_case("job canceled") {
                return Err(JobError::Canceled);
            }
            return Err(JobError::Depot(error));
        }
        Ok(())
    }

    fn download_pack_task_to_direct(
        &self,
        staged_chunks_root: &Path,
        stage: &DirectStagePlan,
        task: &PackDownloadTask,
        task_id: &str,
        control: &JobControl,
        progress_tx: &mpsc::Sender<Result<DownloadProgress, String>>,
    ) -> Result<(), JobError> {
        let relative_path = format!("packs/{}.bin", task.pack_id);
        let partial_path = partial_range_path(staged_chunks_root, task);
        let range = self.fetch_pack_span_with_progress(
            &task.pack_id,
            task.range_start,
            task.range_end,
            &relative_path,
            task_id,
            &partial_path,
            control,
            progress_tx,
        )?;
        if let Err(error) = stage.write_transport_chunks(&task.chunks, task.range_start, &range) {
            let _ = fs::remove_file(&partial_path);
            let _ = fs::remove_file(partial_checkpoint_path(&partial_path));
            return Err(error);
        }
        let _ = fs::remove_file(&partial_path);
        let _ = fs::remove_file(partial_checkpoint_path(&partial_path));
        progress_tx
            .send(Ok(DownloadProgress {
                task_id: task_id.to_string(),
                committed_bytes: task.range_end.saturating_sub(task.range_start),
                in_flight_bytes: 0,
                clear_in_flight: true,
                retry_count: 0,
                rate_bytes_per_second: 0,
                retry_wait_ms: 0,
                rate_limit_wait_ms: 0,
            }))
            .map_err(|error| JobError::Depot(error.to_string()))
    }
}

fn direct_stage_path(staging_root: &Path, file: &FileEntry) -> Result<PathBuf, JobError> {
    let base = safe_join(staging_root, &file.path)
        .ok_or_else(|| JobError::Depot(format!("unsafe staging path: {}", file.path)))?;
    sibling_path(&base, DIRECT_STAGE_SUFFIX)
}

fn direct_plan_id(files: &[FileEntry]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update(file.size.to_le_bytes());
        hasher.update(file.sha256.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn unique_chunks(files: &[FileEntry]) -> Vec<ChunkRef> {
    let mut seen = HashSet::new();
    files
        .iter()
        .flat_map(|file| file.chunks.iter())
        .filter(|chunk| seen.insert(chunk.hash.clone()))
        .cloned()
        .collect()
}

fn load_direct_state(path: &Path) -> Option<DirectStageState> {
    let bytes = fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_stage_fans_out_and_commits_verified_file() {
        let root = env::temp_dir().join(format!(
            "0xolemon-direct-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let downloading = root.join("downloading");
        let staging = downloading.join("files");
        let install = root.join("common");
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir(&install).unwrap();

        let data = b"direct-stage";
        let chunk = ChunkRef {
            hash: blake3::hash(data).to_hex().to_string(),
            file_offset: 0,
            uncompressed_size: data.len() as u64,
            pack_id: "pack-00000".to_string(),
            pack_offset: 0,
            compressed_size: data.len() as u64,
            compressed_sha256: sha256_bytes(data),
            codec: ChunkCodec::Raw,
            encryption: None,
        };
        let file = FileEntry {
            path: "game.bin".to_string(),
            size: data.len() as u64,
            sha256: sha256_bytes(data),
            chunks: vec![chunk.clone()],
            executable: false,
        };
        let stage =
            DirectStagePlan::prepare(&downloading, &staging, std::slice::from_ref(&file), "v2")
                .unwrap();
        stage
            .write_transport_chunks(std::slice::from_ref(&chunk), 0, data)
            .unwrap();
        stage
            .commit_files(&install, std::slice::from_ref(&file))
            .unwrap();
        assert_eq!(fs::read(install.join("game.bin")).unwrap(), data);

        fs::remove_file(install.join("game.bin")).unwrap();
        fs::remove_dir(&install).unwrap();
        fs::remove_dir(&staging).unwrap();
        fs::remove_dir(&downloading).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn failed_multi_file_commit_restores_originals() {
        let root = env::temp_dir().join(format!(
            "0xolemon-rollback-{}",
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        fs::create_dir(&root).unwrap();
        let replaced_target = root.join("replaced.bin");
        let replaced_backup = root.join("replaced.bin.bak");
        let new_target = root.join("new.bin");
        let new_backup = root.join("new.bin.bak");
        fs::write(&replaced_target, b"new").unwrap();
        fs::write(&replaced_backup, b"old").unwrap();
        fs::write(&new_target, b"new-only").unwrap();

        rollback_direct_commits(&[
            (replaced_target.clone(), replaced_backup.clone(), true),
            (new_target.clone(), new_backup, false),
        ]);

        assert_eq!(fs::read(&replaced_target).unwrap(), b"old");
        assert!(!replaced_backup.exists());
        assert!(!new_target.exists());
        fs::remove_file(replaced_target).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
