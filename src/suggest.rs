use crate::db::SuggestionDb;
use crate::state::AppState;
use regex::Regex;
use std::sync::OnceLock;

const MIN_PREFIX_LEN: usize = 2;

fn word_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"[\p{L}\p{N}]+(?:['\-][\p{L}\p{N}]+)?").unwrap())
}

fn normalize_spaces(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\s+").unwrap());
    re.replace_all(s.trim(), " ").to_string()
}

/// Extract the last N words from the typed buffer using Unicode-aware regex
fn extract_context_words(text: &str) -> Vec<String> {
    word_re()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .collect()
}

/// Main suggestion function using n-gram priority:
/// 1. Trigram context (last 2 words + prefix)
/// 2. Bigram context (last 1 word + prefix)
/// 3. Unigram prefix fallback
pub fn refresh_suggestion(db: &SuggestionDb, state: &mut AppState) {
    let normalized = normalize_spaces(&state.typed_buffer.to_lowercase());

    if normalized.is_empty() {
        state.clear_suggestion();
        return;
    }

    let words = extract_context_words(&normalized);
    if words.is_empty() {
        state.clear_suggestion();
        return;
    }

    // Check if user is mid-word (no trailing space) or starting new word (trailing space)
    let ends_with_space = normalized.ends_with(' ');

    if ends_with_space {
        // User just finished a word, predict the NEXT word
        let predicted = predict_next_word(db, &words, "");
        if let Some(word) = predicted {
            state.current_full = Some(format!("{} {}", state.typed_buffer.trim_end(), &word));
            state.current_suffix = Some(word);
            return;
        }
    } else {
        // User is mid-word, complete the current word using n-gram context
        let current_prefix = words.last().unwrap();
        if current_prefix.len() < MIN_PREFIX_LEN {
            state.clear_suggestion();
            return;
        }

        let context_words: Vec<&str> = if words.len() > 1 {
            words[..words.len() - 1].iter().map(|s| s.as_str()).collect()
        } else {
            Vec::new()
        };

        let predicted = predict_next_word(db, &context_words.iter().map(|s| s.to_string()).collect::<Vec<_>>(), current_prefix);
        if let Some(word) = predicted {
            let suffix = &word[current_prefix.len()..];
            if !suffix.is_empty() {
                state.current_full = Some(word.clone());
                state.current_suffix = Some(suffix.to_string());
                return;
            }
        }
    }

    state.clear_suggestion();
}

/// Predict the next word using n-gram hierarchy
fn predict_next_word(db: &SuggestionDb, context: &[String], prefix: &str) -> Option<String> {
    // 1. Trigram: use last 2 context words
    if context.len() >= 2 {
        let w1 = &context[context.len() - 2];
        let w2 = &context[context.len() - 1];
        let results = db.trigram_predict(w1, w2, prefix, 1);
        if let Some((word, _freq)) = results.first() {
            return Some(word.clone());
        }
    }

    // 2. Bigram: use last 1 context word
    if !context.is_empty() {
        let w1 = &context[context.len() - 1];
        let results = db.bigram_predict(w1, prefix, 1);
        if let Some((word, _freq)) = results.first() {
            return Some(word.clone());
        }
    }

    // 3. Unigram: prefix completion
    if prefix.len() >= MIN_PREFIX_LEN {
        let results = db.unigram_predict(prefix, 1);
        if let Some((word, _freq)) = results.first() {
            return Some(word.clone());
        }
    }

    None
}

/// No longer learns from typing. Just clears the buffer.
pub fn finalize_phrase_and_word(_db: &SuggestionDb, state: &mut AppState) {
    state.typed_buffer.clear();
    state.clear_suggestion();
}
