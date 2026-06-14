use std::env;
use std::path::PathBuf;

use first_light_launcher::asset_pack::{
    build_default_pack, default_asset_source, default_pack_output,
};

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let source = take_arg(&args, "--source")
        .map(PathBuf::from)
        .unwrap_or_else(default_asset_source);
    let output = take_arg(&args, "--output")
        .map(PathBuf::from)
        .unwrap_or_else(default_pack_output);

    match build_default_pack(&source, &output) {
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
