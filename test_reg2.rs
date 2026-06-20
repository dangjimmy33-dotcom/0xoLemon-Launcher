fn registry_string(key: &str, value: &str) -> Option<String> {
    let mut command = std::process::Command::new("reg.exe");
    let output = command.args(["query", key, "/v", value]).output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(index) = line.find("REG_SZ") {
            return Some(line[index + "REG_SZ".len()..].trim().to_string());
        }
    }
    None
}

fn main() {
    let v = registry_string("HKCU\\Software\\Valve\\Steam\\Apps\\480", "Name");
    println!("registry_string returned: {:?}", v);
    println!("is_valid: {}", v.is_some_and(|name| name.trim().eq_ignore_ascii_case("Spacewar")));
}
