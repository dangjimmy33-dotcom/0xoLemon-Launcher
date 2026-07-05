use std::path::PathBuf;
fn main() {
    let p = PathBuf::from("c:/program files (x86)/steam").join("config").join("stplug-in");
    println!("Exists: {}", p.exists());
}
