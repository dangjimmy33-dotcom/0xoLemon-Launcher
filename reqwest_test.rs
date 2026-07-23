use reqwest;

#[tokio::main]
async fn main() {
    let url = "https://cdn.0xolemon.com/patches/2.2081 (Build 21578706) - Uploaded 2026-06-16/manifest.json";
    let client = reqwest::Client::new();
    match client.head(url).send().await {
        Ok(res) => println!("Success: {}", res.status()),
        Err(e) => println!("Error: {}", e),
    }
}
