use first_light_launcher::manifest::Catalog;
fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: test_catalog <catalog.json>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        eprintln!("Could not read {path}: {error}");
        std::process::exit(1);
    });
    match serde_json::from_slice::<Catalog>(&bytes) {
        Ok(c) => println!("OK, versions: {}", c.versions.len()),
        Err(e) => println!("ERROR: {}", e),
    }
}
