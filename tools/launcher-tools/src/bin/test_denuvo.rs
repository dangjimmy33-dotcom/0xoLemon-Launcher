use std::path::PathBuf;

fn main() {
    let Some(game_dir) = std::env::args().nth(1).map(PathBuf::from) else {
        eprintln!("Usage: test_denuvo <ea-sports-fc-26-directory>");
        std::process::exit(2);
    };
    let executable = game_dir.join("FC26.exe");
    let config = game_dir.join("anadius.cfg");
    if !executable.is_file() || !config.is_file() {
        eprintln!("FC26.exe and anadius.cfg must both exist directly under the game directory.");
        std::process::exit(1);
    }
    let content = std::fs::read_to_string(&config).unwrap_or_else(|error| {
        eprintln!("Could not read {}: {error}", config.display());
        std::process::exit(1);
    });
    if !content
        .lines()
        .any(|line| line.trim_start().starts_with("\"DenuvoToken\""))
    {
        eprintln!("anadius.cfg does not contain a DenuvoToken field.");
        std::process::exit(1);
    }
    println!(
        "Offline activation prerequisites are present in {}.",
        game_dir.display()
    );
}
