use std::process::Command;
fn main() {
    let output = Command::new("reg.exe").args(["query", "HKCU\\Software\\Valve\\Steam\\Apps\\480", "/v", "Name"]).output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(index) = line.find("REG_SZ") {
            println!("found: '{}'", line[index + "REG_SZ".len()..].trim());
        }
    }
}
