use std::process::Command;
fn main() {
    let url = "https://example.com/test?a=1&b=2".replace("&", "^&");
    let output = Command::new("cmd").args(["/C", "echo", "", &url]).output().unwrap();
    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));
}
