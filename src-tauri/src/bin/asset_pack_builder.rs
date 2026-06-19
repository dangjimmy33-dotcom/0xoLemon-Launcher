use std::env;
use std::path::PathBuf;

use first_light_launcher::asset_pack::{
    build_all_packs, default_asset_source, default_pack_output,
};

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let source = take_arg(&args, "--source")
        .map(PathBuf::from)
        .unwrap_or_else(default_asset_source);
    let output = take_arg(&args, "--output")
        .map(PathBuf::from)
        .unwrap_or_else(default_pack_output);

    let mut sources = Vec::new();
    let source_is_game = source
        .join("details")
        .join("metadata")
        .join("game-detail.normalized.json")
        .is_file();
    if source_is_game {
        sources.push(source.clone());
    } else if let Ok(entries) = std::fs::read_dir(&source) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                let name = entry.file_name().to_string_lossy().to_string();
                if name != "fonts" && name != "images" {
                    sources.push(entry.path());
                }
            }
        }
    } else {
        sources.push(source);
    }

    match build_all_packs(&sources, &output) {
        Ok(summary) => {
            println!(
                "built {} with {} game(s), {} asset(s), {} achievement image(s)",
                summary.output_path,
                summary.game_count,
                summary.asset_count,
                summary.achievement_count
            );
        }
        Err(error) => {
            eprintln!("asset pack build failed: {error}");
            std::process::exit(1);
        }
    }
}

fn take_arg(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}
