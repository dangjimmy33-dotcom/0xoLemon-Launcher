use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;

fn registered_commands(source: &str) -> Result<BTreeSet<String>, String> {
    let (_, handler) = source
        .split_once("generate_handler![")
        .ok_or_else(|| "generate_handler! block was not found in src/lib.rs".to_string())?;
    let (handler, _) = handler
        .split_once("]);")
        .ok_or_else(|| "generate_handler! block is not terminated in src/lib.rs".to_string())?;

    Ok(handler
        .lines()
        .filter_map(|line| {
            let code = line.split("//").next()?.trim();
            if !code.ends_with(',') {
                return None;
            }

            let command = code.trim_end_matches(',').trim().rsplit("::").next()?;
            command
                .chars()
                .all(|character| character == '_' || character.is_ascii_alphanumeric())
                .then(|| command.to_string())
        })
        .collect())
}

fn allowed_commands(permission_path: &Path) -> Result<BTreeSet<String>, String> {
    let content = fs::read_to_string(permission_path)
        .map_err(|error| format!("could not read {}: {error}", permission_path.display()))?;
    let document: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| format!("invalid {}: {error}", permission_path.display()))?;

    let mut allowed = BTreeSet::new();
    let permissions = document
        .get("permission")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("{} has no permission array", permission_path.display()))?;

    for permission in permissions {
        if let Some(commands) = permission
            .get("commands")
            .and_then(|commands| commands.get("allow"))
            .and_then(serde_json::Value::as_array)
        {
            for command in commands.iter().filter_map(serde_json::Value::as_str) {
                allowed.insert(command.to_string());
            }
        }
    }
    Ok(allowed)
}

fn validate_command_acl(manifest_dir: &Path) -> Result<(), String> {
    let lib_path = manifest_dir.join("src/lib.rs");
    let permission_path = manifest_dir.join("permissions/allow-all.json");
    let source = fs::read_to_string(&lib_path)
        .map_err(|error| format!("could not read {}: {error}", lib_path.display()))?;
    let registered = registered_commands(&source)?;
    let allowed = allowed_commands(&permission_path)?;
    let missing = registered.difference(&allowed).cloned().collect::<Vec<_>>();

    if missing.is_empty() {
        return Ok(());
    }

    Err(format!(
        "commands registered in generate_handler! but missing from permissions/allow-all.json: {}",
        missing.join(", ")
    ))
}

fn main() {
    println!("cargo:rerun-if-env-changed=OXO_DISCORD_CLIENT_ID");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=permissions/allow-all.json");

    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .expect("CARGO_MANIFEST_DIR is unavailable");
    if let Err(error) = validate_command_acl(&manifest_dir) {
        panic!("Tauri command ACL validation failed: {error}");
    }

    tauri_build::build();
}
