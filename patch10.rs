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