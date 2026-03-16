use crate::ngram_cache::NgramCache;
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

fn extract_context_words(text: &str) -> Vec<String> {
    word_re()
        .find_iter(text)
        .map(|m| m.as_str().to_lowercase())
        .collect()
}

/// Main suggestion function using in-memory n-gram cache.
/// Priority: trigram > bigram > unigram prefix
pub fn refresh_suggestion(cache: &NgramCache, state: &mut AppState) {
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

    let ends_with_space = normalized.ends_with(' ');

    if ends_with_space {
        // Predict the NEXT word
        if let Some(word) = predict_next_word(cache, &words, "") {
            state.current_full = Some(format!("{} {}", state.typed_buffer.trim_end(), &word));
            state.current_suffix = Some(word);
            return;
        }
    } else {
        // Complete the current word
        let current_prefix = words.last().unwrap();
        if current_prefix.len() < MIN_PREFIX_LEN {
            state.clear_suggestion();
            return;
        }

        let context: Vec<&str> = if words.len() > 1 {
            words[..words.len() - 1].iter().map(|s| s.as_str()).collect()
        } else {
            Vec::new()
        };

        if let Some(word) = predict_with_context(cache, &context, current_prefix) {
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

fn predict_next_word(cache: &NgramCache, context: &[String], prefix: &str) -> Option<String> {
    if context.len() >= 2 {
        let w1 = &context[context.len() - 2];
        let w2 = &context[context.len() - 1];
        if let Some(word) = cache.trigram_predict(w1, w2, prefix) {
            return Some(word);
        }
    }
    if !context.is_empty() {
        let w1 = &context[context.len() - 1];
        if let Some(word) = cache.bigram_predict(w1, prefix) {
            return Some(word);
        }
    }
    if prefix.len() >= MIN_PREFIX_LEN {
        return cache.unigram_predict(prefix);
    }
    None
}

fn predict_with_context(cache: &NgramCache, context: &[&str], prefix: &str) -> Option<String> {
    if context.len() >= 2 {
        let w1 = context[context.len() - 2];
        let w2 = context[context.len() - 1];
        if let Some(word) = cache.trigram_predict(w1, w2, prefix) {
            return Some(word);
        }
    }
    if !context.is_empty() {
        let w1 = context[context.len() - 1];
        if let Some(word) = cache.bigram_predict(w1, prefix) {
            return Some(word);
        }
    }
    if prefix.len() >= MIN_PREFIX_LEN {
        return cache.unigram_predict(prefix);
    }
    None
}

/// Just clears the buffer. No learning from key events.
pub fn finalize_buffer(state: &mut AppState) {
    state.typed_buffer.clear();
    state.clear_suggestion();
}
