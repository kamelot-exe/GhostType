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
    date_unixtime: Option<String>,
    text: serde_json::Value,
}

fn word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[\p{L}\p{N}]+(?:['-][\p{L}\p{N}]+)?").unwrap())
}

fn extract_words(text: &str) -> Vec<String> {
    word_re()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .collect()
}

pub fn import_default_files(db: &SuggestionDb) {
    let files = ["db/db1.json", "db/db2.json", "db/db3.json", "db/db4.json"];

    for path in files {
        match import_one(db, path) {
            Ok((m, w, p)) => {
                println!("[ok] {path}: messages={m}, words={w}, phrases={p}");
            }
            Err(e) => {
                println!("[skip] {path}: {e}");
            }
        }
    }
}

fn import_one(db: &SuggestionDb, path: &str) -> Result<(usize, usize, usize), String> {
    if !Path::new(path).exists() {
        return Err("file not found".into());
    }

    let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let export: ExportRoot = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let mut used_messages = 0usize;
    let mut inserted_words = 0usize;
    let mut inserted_phrases = 0usize;

    for msg in export.messages {
        if msg.msg_type.as_deref() != Some("message") {
            continue;
        }
        if msg.from_id.as_deref() != Some(TARGET_FROM_ID) {
            continue;
        }

        let text = match msg.text {
            serde_json::Value::String(s) => s,
            _ => continue,
        };

        let words = extract_words(&text);
        if words.is_empty() {
            continue;
        }

        used_messages += 1;
        let ts = msg
            .date_unixtime
            .as_deref()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        let mut word_counts: HashMap<String, i64> = HashMap::new();
        for w in &words {
            *word_counts.entry(w.clone()).or_insert(0) += 1;
        }

        for (word, count) in word_counts {
            let _ = db.add_word_with_ts(&word, ts, count);
            inserted_words += count as usize;
        }

        let mut phrase_counts: HashMap<String, i64> = HashMap::new();
        for n in [2usize, 3usize, 4usize] {
            if words.len() < n {
                continue;
            }
            for i in 0..=(words.len() - n) {
                let phrase = words[i..i + n].join(" ");
                *phrase_counts.entry(phrase).or_insert(0) += 1;
            }
        }

        for (phrase, count) in phrase_counts {
            let _ = db.add_phrase_with_ts(&phrase, ts, count);
            inserted_phrases += count as usize;
        }
    }

    Ok((used_messages, inserted_words, inserted_phrases))
}