use crate::db::SuggestionDb;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

const TARGET_FROM_ID: &str = "user7695237555";

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

pub fn import_default_files(db: &SuggestionDb) {
    let files = ["db/db1.json", "db/db2.json", "db/db3.json", "db/db4.json"];

    for path in files {
        if !Path::new(path).exists() {
            println!("[skip] {path}: file not found");
            continue;
        }
        println!("Importing {path}...");
        match import_one(db, path) {
            Ok(stats) => {
                println!(
                    "[ok] {path}: Imported {} messages, {} unigrams, {} bigrams, {} trigrams",
                    stats.messages, stats.unigrams, stats.bigrams, stats.trigrams
                );
            }
            Err(e) => {
                println!("[error] {path}: {e}");
            }
        }
    }
}

pub fn import_file(db: &SuggestionDb, path: &str) -> Result<ImportStats, String> {
    if !Path::new(path).exists() {
        return Err("file not found".into());
    }
    println!("Importing {path}...");
    import_one(db, path)
}

pub struct ImportStats {
    pub messages: usize,
    pub unigrams: usize,
    pub bigrams: usize,
    pub trigrams: usize,
}

fn import_one(db: &SuggestionDb, path: &str) -> Result<ImportStats, String> {
    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let export: ExportRoot = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let mut used_messages = 0usize;
    let mut unigram_counts: HashMap<String, i64> = HashMap::new();
    let mut bigram_counts: HashMap<(String, String), i64> = HashMap::new();
    let mut trigram_counts: HashMap<(String, String, String), i64> = HashMap::new();

    for msg in &export.messages {
        if msg.msg_type.as_deref() != Some("message") {
            continue;
        }
        if msg.from_id.as_deref() != Some(TARGET_FROM_ID) {
            continue;
        }

        let text = match &msg.text {
            serde_json::Value::String(s) => s.clone(),
            _ => continue,
        };

        let words = extract_words(&text);
        if words.is_empty() {
            continue;
        }

        used_messages += 1;

        // Unigrams
        for w in &words {
            *unigram_counts.entry(w.clone()).or_insert(0) += 1;
        }

        // Bigrams
        for pair in words.windows(2) {
            let key = (pair[0].clone(), pair[1].clone());
            *bigram_counts.entry(key).or_insert(0) += 1;
        }

        // Trigrams
        for triple in words.windows(3) {
            let key = (triple[0].clone(), triple[1].clone(), triple[2].clone());
            *trigram_counts.entry(key).or_insert(0) += 1;
        }
    }

    // Batch insert into DB
    db.batch_insert_unigrams(&unigram_counts).map_err(|e| e.to_string())?;
    db.batch_insert_bigrams(&bigram_counts).map_err(|e| e.to_string())?;
    db.batch_insert_trigrams(&trigram_counts).map_err(|e| e.to_string())?;

    Ok(ImportStats {
        messages: used_messages,
        unigrams: unigram_counts.len(),
        bigrams: bigram_counts.len(),
        trigrams: trigram_counts.len(),
    })
}
