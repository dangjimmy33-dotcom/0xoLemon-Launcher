use std::env;
use std::path::PathBuf;

use first_light_launcher::builder::{
    build_depot, BuildDepotInput, BuildVersionInput, DepotEncryptionConfig, PublishTarget,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("depot_builder error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("build-pair") => build_pair(&args),
        Some("build-version") => build_version(&args),
        _ => {
            print_usage();
            Err("expected build-pair or build-version command".to_string())
        }
    }
}

fn encryption_config(args: &[String]) -> DepotEncryptionConfig {
    let disabled = has_flag(args, "--no-encrypt-packs") || has_flag(args, "--plain-packs");
    let explicitly_enabled = has_flag(args, "--encrypt-packs");
    let config = DepotEncryptionConfig {
        enabled: explicitly_enabled || !disabled,
        key_material: take_arg(args, "--encryption-key")
            .ok()
            .or_else(|| take_arg(args, "--depot-key").ok()),
        key_id: take_arg(args, "--encryption-key-id").ok(),
    };
    eprintln!(
        "[DEPOT] transport encryption: {}",
        if config.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    config
}

fn pack_target_size(args: &[String]) -> u64 {
    let mb = take_arg(args, "--pack-target-mb")
        .or_else(|_| take_arg(args, "--pack-size-mb"))
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(256);
    let mb = mb.clamp(4, 4096);
    mb * 1024 * 1024
}

fn pack_id_prefix(args: &[String]) -> String {
    take_arg(args, "--pack-id-prefix")
        .or_else(|_| take_arg(args, "--pack-prefix"))
        .unwrap_or_else(|_| "pack-".to_string())
}

fn pack_start_index(args: &[String]) -> Option<usize> {
    take_arg(args, "--pack-start-index")
        .or_else(|_| take_arg(args, "--pack-start"))
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
}

fn depot_format_version(args: &[String]) -> u32 {
    take_arg(args, "--format-version")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(1)
}

fn build_pair(args: &[String]) -> Result<(), String> {
    let old_input = take_arg(&args, "--old-input")?;
    let old_version = take_arg(&args, "--old-version").unwrap_or_else(|_| "v1.0".to_string());
    let new_input = take_arg(&args, "--new-input")?;
    let new_version = take_arg(&args, "--new-version").unwrap_or_else(|_| "v1.1".to_string());
    let output = take_arg(&args, "--out")?;
    let game_id = take_arg(&args, "--game-id").unwrap_or_else(|_| "007-first-light".to_string());
    let upload_repo = take_arg(&args, "--upload-repo").ok();
    let repo_type = take_arg(&args, "--repo-type").unwrap_or_else(|_| "dataset".to_string());
    let repo_prefix = take_arg(&args, "--repo-prefix").unwrap_or_else(|_| game_id.clone());
    let keep_local_packs = has_flag(&args, "--keep-local-packs");
    let extend_existing = has_flag(&args, "--extend-existing");
    let launch_executable = take_arg(&args, "--launch-executable").ok();

    let report = build_depot(BuildDepotInput {
        game_id: game_id.clone(),
        latest_version: new_version.clone(),
        output_dir: PathBuf::from(output),
        versions: vec![
            BuildVersionInput {
                version: old_version,
                root: PathBuf::from(old_input),
                launch_executable: launch_executable.clone(),
            },
            BuildVersionInput {
                version: new_version,
                root: PathBuf::from(new_input),
                launch_executable,
            },
        ],
        publish: upload_repo.map(|repo_id| PublishTarget {
            repo_id,
            repo_type,
            repo_prefix,
            delete_local_packs: !keep_local_packs,
        }),
        extend_existing,
        encryption: encryption_config(args),
        pack_target_size: pack_target_size(args),
        pack_id_prefix: pack_id_prefix(args),
        start_pack_index: pack_start_index(args),
        format_version: depot_format_version(args),
    })
    .map_err(|err| err.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn build_version(args: &[String]) -> Result<(), String> {
    let input = take_arg(&args, "--input")?;
    let version = take_arg(&args, "--version")?;
    let output = take_arg(&args, "--out")?;
    let game_id = take_arg(&args, "--game-id")?;
    let upload_repo = take_arg(&args, "--upload-repo").ok();
    let repo_type = take_arg(&args, "--repo-type").unwrap_or_else(|_| "dataset".to_string());
    let repo_prefix = take_arg(&args, "--repo-prefix").unwrap_or_else(|_| game_id.clone());
    let keep_local_packs = has_flag(&args, "--keep-local-packs");
    let extend_existing = has_flag(&args, "--extend-existing");
    let launch_executable = take_arg(&args, "--launch-executable").ok();

    let report = build_depot(BuildDepotInput {
        game_id: game_id.clone(),
        latest_version: version.clone(),
        output_dir: PathBuf::from(output),
        versions: vec![BuildVersionInput {
            version,
            root: PathBuf::from(input),
            launch_executable,
        }],
        publish: upload_repo.map(|repo_id| PublishTarget {
            repo_id,
            repo_type,
            repo_prefix,
            delete_local_packs: !keep_local_packs,
        }),
        extend_existing,
        encryption: encryption_config(args),
        pack_target_size: pack_target_size(args),
        pack_id_prefix: pack_id_prefix(args),
        start_pack_index: pack_start_index(args),
        format_version: depot_format_version(args),
    })
    .map_err(|err| err.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|arg| arg == name)
}

fn take_arg(args: &[String], name: &str) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("missing {name}"))
}

fn print_usage() {
    eprintln!(
        "usage:\n  depot_builder build-pair --old-input <path> --new-input <path> --out <path> [--old-version v1.0] [--new-version v1.1] [--game-id 007-first-light] [--format-version 1|2] [--launch-executable <relative-exe>] [--upload-repo owner/repo --repo-type dataset --repo-prefix 007-first-light] [--keep-local-packs] [--extend-existing] [--pack-target-mb 128] [--pack-id-prefix pack-] [--pack-start-index 0] [--encrypt-packs|--encryption-key <key>|--no-encrypt-packs]\n  depot_builder build-version --input <path> --version <version> --out <path> --game-id <game-id> [--format-version 1|2] [--launch-executable <relative-exe>] [--upload-repo owner/repo --repo-type dataset --repo-prefix <prefix>] [--keep-local-packs] [--extend-existing] [--pack-target-mb 128] [--pack-id-prefix pack-] [--pack-start-index 0] [--encrypt-packs|--encryption-key <key>|--no-encrypt-packs]"
    );
}
