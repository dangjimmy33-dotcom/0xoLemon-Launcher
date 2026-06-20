fn registry_string(key: &str, value: &str) -> Option<String> {
    let mut command = std::process::Command::new("reg.exe");
    let output = command.args(["query", key, "/v", value]).output().ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(index) = line.find("REG_SZ") {
            return Some(line[index + "REG_SZ".len()..].trim().to_string());
        }
    }
    None
}
fn spacewar_registration_name_is_valid(value: &str) -> bool {
    value.trim().eq_ignore_ascii_case("Spacewar")
}
fn steam_registry_has_spacewar() -> bool {
    registry_string(r"HKCU\Software\Valve\Steam\Apps\480", "Name")
        .is_some_and(|name| spacewar_registration_name_is_valid(&name))
}
fn main() {
    println!("has_spacewar: {}", steam_registry_has_spacewar());
}
