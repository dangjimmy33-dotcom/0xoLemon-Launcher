use first_light_launcher::manifest::Catalog;
fn main() {
    let bytes = std::fs::read("E:/007Launcher/depot/007-first-light/catalog.json").unwrap();
    match serde_json::from_slice::<Catalog>(&bytes) {
        Ok(c) => println!("OK, versions: {}", c.versions.len()),
        Err(e) => println!("ERROR: {}", e),
    }
}
