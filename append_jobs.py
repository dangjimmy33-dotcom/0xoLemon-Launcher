import os

with open('final_funcs.rs', 'r', encoding='utf-8') as f:
    funcs = f.read()

missing_funcs = """
pub fn spawn_patch_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    install_path: String,
    target_version: Option<String>,
    game_id: Option<String>,
) -> Result<JobJournal, JobError> {
    let source = DepotSource::for_game(game_id.as_deref().unwrap_or(DEFAULT_GAME_ID));
    let install_root = std::path::PathBuf::from(&install_path);
    let target_version = resolve_patch_target_version(&source, &install_root, target_version)?;

    let journal = default_patch_journal(&source, install_path, &target_version);

    let app_for_thread = app.clone();
    let control_for_thread = control.clone();
    let initial = journal.clone();
    
    std::thread::spawn(move || {
        let result = run_real_patch_job(&app_for_thread, control_for_thread.clone(), initial.clone());
        match result {
            Ok(final_journal) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &final_journal.id);
            }
            Err(JobError::Cancelled(canceled_job_id)) => {
                let _ = clear_current_journal_if_matches(&app_for_thread, &canceled_job_id);
            }
            Err(e) => {
                let mut errored = initial.clone();
                errored.status = JobStatus::Failed;
                append_log(&mut errored, "error", &format!("Patch failed: {}", e));
                let _ = persist_and_emit(&app_for_thread, &errored);
                let _ = clear_current_journal_if_matches(&app_for_thread, &errored.id);
            }
        }
    });

    persist_and_emit(app, &journal)?;
    Ok(journal)
}

fn run_real_patch_job(
    app: &AppHandle,
    control: Arc<JobControl>,
    mut journal: JobJournal,
) -> Result<JobJournal, JobError> {
    let source = DepotSource::for_game(&journal.game_id);
    let install_root = std::path::PathBuf::from(&journal.install_path);
    
    set_step_running(app, &mut journal, 0, JobStatus::Running, "Check patch")?;
    resolve_patch_target_version(&source, &install_root, Some(journal.to_version.clone()))?;
    complete_step(app, &mut journal, 0)?;

    let to_version = journal.to_version.clone();
    try_apply_patch_fix(app, &mut journal, &source, &install_root, &to_version, &control, true)?;

    journal.status = JobStatus::Committed;
    persist_and_emit(app, &journal)?;
    Ok(journal)
}

fn resolve_patch_target_version(
    source: &DepotSource,
    install_root: &std::path::Path,
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
"""

with open('src-tauri/src/job.rs', 'a', encoding='utf-8') as f:
    f.write('\n' + missing_funcs + '\n' + funcs + '\n')

with open('src-tauri/src/job.rs', 'r', encoding='utf-8') as f:
    content = f.read()

content = content.replace(
    'let is_real = journal.kind == "install" || journal.kind == "update" || journal.kind == "repair";',
    'let is_real = matches!(journal.kind.as_str(), "install" | "update" | "repair" | "patch");'
)

with open('src-tauri/src/job.rs', 'w', encoding='utf-8') as f:
    f.write(content)

