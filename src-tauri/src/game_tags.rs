use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

const GAME_TAGS_JSON: &str = include_str!("../game-tags.json");

#[derive(Debug, Deserialize)]
struct GameTagTable {
    #[allow(dead_code)]
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    games: HashMap<String, Vec<String>>,
}

fn table() -> &'static GameTagTable {
    static TABLE: OnceLock<GameTagTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(GAME_TAGS_JSON).unwrap_or_else(|_| GameTagTable {
            schema_version: 1,
            games: HashMap::new(),
        })
    })
}

pub fn game_has_tag(game_id: &str, tag_id: &str) -> bool {
    table()
        .games
        .get(game_id)
        .map(|tags| tags.iter().any(|tag| tag.eq_ignore_ascii_case(tag_id)))
        .unwrap_or(false)
}
