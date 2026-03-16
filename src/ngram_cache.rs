use crate::db::SuggestionDb;
use std::collections::HashMap;

/// In-memory cache for n-gram lookups to avoid hitting SQLite on every keypress.
/// Loaded once from DB and refreshed on import/rebuild.
pub struct NgramCache {
    /// word -> freq
    pub unigrams: HashMap<String, i64>,
    /// (w1, w2) -> vec of (w3, freq) sorted by freq desc
    pub trigrams: HashMap<(String, String), Vec<(String, i64)>>,
    /// w1 -> vec of (w2, freq) sorted by freq desc
    pub bigrams: HashMap<String, Vec<(String, i64)>>,
    /// prefix -> vec of (word, freq) sorted by freq desc (for unigram completion)
    prefix_cache: HashMap<String, Vec<(String, i64)>>,
}

impl NgramCache {
    pub fn load_from_db(db: &SuggestionDb) -> Self {
        let mut cache = Self {
            unigrams: HashMap::new(),
            trigrams: HashMap::new(),
            bigrams: HashMap::new(),
            prefix_cache: HashMap::new(),
        };
        cache.refresh(db);
        cache
    }

    pub fn refresh(&mut self, db: &SuggestionDb) {
        self.unigrams = db.load_all_unigrams();
        self.bigrams = db.load_all_bigrams();
        self.trigrams = db.load_all_trigrams();
        self.build_prefix_cache();
    }

    fn build_prefix_cache(&mut self) {
        self.prefix_cache.clear();
        for (word, freq) in &self.unigrams {
            if word.len() < 2 {
                continue;
            }
            // Index by all prefixes of length 2+
            for end in 2..=word.len() {
                if word.is_char_boundary(end) {
                    let prefix = &word[..end];
                    self.prefix_cache
                        .entry(prefix.to_string())
                        .or_default()
                        .push((word.clone(), *freq));
                }
            }
        }
        // Sort each prefix bucket by frequency desc
        for entries in self.prefix_cache.values_mut() {
            entries.sort_by(|a, b| b.1.cmp(&a.1));
            entries.truncate(5); // Keep top 5 per prefix
        }
    }

    /// Trigram prediction: given w1, w2, find best w3 starting with prefix
    pub fn trigram_predict(&self, w1: &str, w2: &str, prefix: &str) -> Option<String> {
        let key = (w1.to_string(), w2.to_string());
        if let Some(entries) = self.trigrams.get(&key) {
            for (w3, _freq) in entries {
                if prefix.is_empty() || w3.starts_with(prefix) {
                    return Some(w3.clone());
                }
            }
        }
        None
    }

    /// Bigram prediction: given w1, find best w2 starting with prefix
    pub fn bigram_predict(&self, w1: &str, prefix: &str) -> Option<String> {
        if let Some(entries) = self.bigrams.get(w1) {
            for (w2, _freq) in entries {
                if prefix.is_empty() || w2.starts_with(prefix) {
                    return Some(w2.clone());
                }
            }
        }
        None
    }

    /// Unigram prefix completion
    pub fn unigram_predict(&self, prefix: &str) -> Option<String> {
        if let Some(entries) = self.prefix_cache.get(prefix) {
            if let Some((word, _)) = entries.first() {
                if word != prefix {
                    return Some(word.clone());
                }
                // If first match is exact, return second
                if entries.len() > 1 {
                    return Some(entries[1].0.clone());
                }
            }
        }
        None
    }
}
