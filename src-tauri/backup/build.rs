fn main() {
    println!("cargo:rerun-if-env-changed=OXO_DISCORD_CLIENT_ID");
    tauri_build::build();
}
