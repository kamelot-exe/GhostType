use crate::db::SuggestionDb;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

// Embedded Telegram JSON exports — compiled into the binary.
const EMBEDDED: &[&[u8]] = &[
    include_bytes!("data/db1.json"),
    include_bytes!("data/db2.json"),
    include_bytes!("data/db3.json"),
    include_bytes!("data/db4.json"),
];

#[derive(Debug, Deserialize)]
struct ExportRoot {
    messages: Vec<TgMessage>,
}

#[derive(Debug, Deserialize)]
struct TgMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    from_id: Option<String>,
    #[allow(dead_code)]
    date_unixtime: Option<String>,
    text: serde_json::Value,
}

fn word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[\p{L}\p{N}]+(?:['\-][\p{L}\p{N}]+)?").unwrap())
}

fn extract_words(text: &str) -> Vec<String> {
    word_re()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .filter(|w| w.len() >= 2)
        .collect()
}

pub struct ImportStats {
    pub messages: usize,
    pub unigrams: usize,
    pub bigrams: usize,
    pub trigrams: usize,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Import all four embedded datasets (compiled into the binary).
/// Call this on first run when the DB is empty.
pub fn import_embedded(db: &SuggestionDb) -> ImportStats {
    let mut total = ImportStats { messages: 0, unigrams: 0, bigrams: 0, trigrams: 0 };
    for (i, &bytes) in EMBEDDED.iter().enumerate() {
        match std::str::from_utf8(bytes) {
            Ok(s) => match import_from_str(db, s, None) {
                Ok(stats) => {
                    println!(
                        "  [embedded db{}] {} msgs, {} uni, {} bi, {} tri",
                        i + 1, stats.messages, stats.unigrams, stats.bigrams, stats.trigrams
                    );
                    total.messages += stats.messages;
                    total.unigrams += stats.unigrams;
                    total.bigrams += stats.bigrams;
                    total.trigrams += stats.trigrams;
                }
                Err(e) => eprintln!("  [embedded db{}] parse error: {e}", i + 1),
            },
            Err(e) => eprintln!("  [embedded db{}] utf8 error: {e}", i + 1),
        }
    }
    total
}

/// Import a JSON file from disk, optionally filtered to a specific from_id.
pub fn import_file(db: &SuggestionDb, path: &str) -> Result<ImportStats, String> {
    if !Path::new(path).exists() {
        return Err("file not found".into());
    }
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    import_from_str(db, &raw, None)
}

/// Import the legacy `db/db*.json` files from disk (if present).
pub fn import_default_files(db: &SuggestionDb) {
    for path in &["db/db1.json", "db/db2.json", "db/db3.json", "db/db4.json"] {
        if !Path::new(path).exists() {
            println!("[skip] {path}: file not found");
            continue;
        }
        let raw = match fs::read_to_string(path) {
            Ok(r) => r,
            Err(e) => { eprintln!("[error] {path}: {e}"); continue; }
        };
        match import_from_str(db, &raw, None) {
            Ok(stats) => println!(
                "[ok] {path}: {} msgs, {} uni, {} bi, {} tri",
                stats.messages, stats.unigrams, stats.bigrams, stats.trigrams
            ),
            Err(e) => eprintln!("[error] {path}: {e}"),
        }
    }
}

// ── Core import ───────────────────────────────────────────────────────────────

/// Parse Telegram JSON and insert n-grams into DB.
/// `from_id_filter`: if Some, only import messages from that user ID.
/// If None, imports all "message" type entries regardless of sender.
fn import_from_str(
    db: &SuggestionDb,
    json: &str,
    from_id_filter: Option<&str>,
) -> Result<ImportStats, String> {
    let export: ExportRoot = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let mut used = 0usize;
    let mut uni: HashMap<String, i64> = HashMap::new();
    let mut bi: HashMap<(String, String), i64> = HashMap::new();
    let mut tri: HashMap<(String, String, String), i64> = HashMap::new();

    for msg in &export.messages {
        if msg.msg_type.as_deref() != Some("message") {
            continue;
        }
        if let Some(filter) = from_id_filter {
            if msg.from_id.as_deref() != Some(filter) {
                continue;
            }
        }

        let text = match &msg.text {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(arr) => {
                // Telegram sometimes encodes formatted text as an array
                arr.iter()
                    .filter_map(|v| match v {
                        serde_json::Value::String(s) => Some(s.as_str()),
                        serde_json::Value::Object(o) => {
                            o.get("text").and_then(|t| t.as_str())
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("")
            }
            _ => continue,
        };

        let words = extract_words(&text);
        if words.is_empty() {
            continue;
        }
        used += 1;

        for w in &words {
            *uni.entry(w.clone()).or_insert(0) += 1;
        }
        for pair in words.windows(2) {
            *bi.entry((pair[0].clone(), pair[1].clone())).or_insert(0) += 1;
        }
        for triple in words.windows(3) {
            *tri.entry((triple[0].clone(), triple[1].clone(), triple[2].clone()))
                .or_insert(0) += 1;
        }
    }

    db.batch_insert_unigrams(&uni).map_err(|e| e.to_string())?;
    db.batch_insert_bigrams(&bi).map_err(|e| e.to_string())?;
    db.batch_insert_trigrams(&tri).map_err(|e| e.to_string())?;

    Ok(ImportStats {
        messages: used,
        unigrams: uni.len(),
        bigrams: bi.len(),
        trigrams: tri.len(),
    })
}
