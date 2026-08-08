use first_light_launcher::denuvo::*;
use std::time::Duration;
use std::path::PathBuf;

fn main() {
    tauri::async_runtime::block_on(async {
        let game_dir = "E:\\Compressed\\FC26_165-001\\UpdatedFiles_20260724_113859".to_string();

        println!("1. Downloading magic file...");
        let on_progress = |_p: DownloadProgress| {};
        let on_extract = || { println!("Extracting..."); };
        download_and_extract_magic_file_inner(game_dir.clone(), on_progress, on_extract).await.unwrap();

        println!("2. Deleting old tickets...");
        delete_denuvo_tickets(game_dir.clone()).await.unwrap();

        println!("3. Launching game to generate ticket (silent)...");
        launch_game_executable(game_dir.clone(), "FC26.exe".to_string(), Some(true)).await.unwrap();

        println!("4. Waiting 6 seconds...");
        std::thread::sleep(Duration::from_secs(6));

        println!("5. Scanning for ticket...");
        let ticket = scan_for_denuvo_ticket(game_dir.clone()).await.unwrap();
        println!("Found ticket: {}...", &ticket[..std::cmp::min(ticket.len(), 50)]);

        println!("6. Getting token from server...");
        let res = get_denuvo_token_from_server(ticket, "http://127.0.0.1:3030".to_string()).await.unwrap();
        assert!(res.success, "Token generation failed: {:?}", res.message);
        println!("Token generated: {}", res.token.as_ref().unwrap());

        println!("7. Applying token to config...");
        let cfg_path = format!("{}\\anadius.cfg", game_dir);
        let cfg_p = PathBuf::from(&cfg_path);
        if !cfg_p.exists() {
            std::fs::write(&cfg_p, "\"DenuvoToken\" \"PLACEHOLDER\"").unwrap();
        }
        apply_denuvo_token_to_cfg(cfg_path, res.token.unwrap()).await.unwrap();

        println!("Flow complete!");
    });
}
