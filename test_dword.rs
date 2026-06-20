fn registry_dword(key: &str, value: &str) -> Option<u32> {
    let mut command = std::process::Command::new("reg.exe");
    let output = command.args(["query", key, "/v", value]).output().ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(index) = line.find("REG_DWORD") {
            let raw = line[index + "REG_DWORD".len()..].trim();
            return raw.strip_prefix("0x").and_then(|value| u32::from_str_radix(value, 16).ok()).or_else(|| raw.parse::<u32>().ok());
        }
    }
    None
}
fn main() {
    println!("registry_dword: {:?}", registry_dword("HKCU\\Software\\Valve\\Steam\\Apps\\480", "Installed"));
}
