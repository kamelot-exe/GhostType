use crate::db::SuggestionDb;
use crate::state::AppState;
use regex::Regex;
use std::sync::OnceLock;

const MIN_WORD_PREFIX_LEN: usize = 2;
const MIN_PHRASE_PREFIX_LEN: usize = 4;

fn normalize_spaces(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\s+").unwrap());
    re.replace_all(s.trim(), " ").to_string()
}

pub fn refresh_suggestion(db: &SuggestionDb, state: &mut AppState) {
    let normalized = normalize_spaces(&state.typed_buffer.to_lowercase());

    if normalized.is_empty() {
        state.clear_suggestion();
        return;
    }

    if normalized.len() >= MIN_PHRASE_PREFIX_LEN {
        if let Ok(Some(phrase)) = db.best_phrase_match(&normalized) {
            if phrase != normalized && phrase.starts_with(&normalized) {
                let suffix = phrase[normalized.len()..].to_string();
                if !suffix.is_empty() {
                    state.current_full = Some(phrase);
                    state.current_suffix = Some(suffix);
                    return;
                }
            }
        }
    }

    let last_word = normalized
        .split(' ')
        .last()
        .unwrap_or_default()
        .trim();

    if last_word.len() >= MIN_WORD_PREFIX_LEN {
        if let Ok(Some(word)) = db.best_word_match(last_word) {
            if word != last_word && word.starts_with(last_word) {
                let suffix = word[last_word.len()..].to_string();
                if !suffix.is_empty() {
                    state.current_full = Some(word);
                    state.current_suffix = Some(suffix);
                    return;
                }
            }
        }
    }

    state.clear_suggestion();
}

pub fn finalize_phrase_and_word(db: &SuggestionDb, state: &mut AppState) {
    let normalized = normalize_spaces(&state.typed_buffer.to_lowercase());
    if normalized.is_empty() {
        state.typed_buffer.clear();
        state.clear_suggestion();
        return;
    }

    if let Some(last_word) = normalized.split(' ').last() {
        if !last_word.is_empty() {
            let _ = db.add_word(last_word);
        }
    }

    if normalized.split(' ').count() >= 2 {
        let _ = db.add_phrase(&normalized);
    }

    state.typed_buffer.clear();
    state.clear_suggestion();
}