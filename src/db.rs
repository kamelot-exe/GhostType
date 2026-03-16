use rusqlite::{params, Connection, Result};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SuggestionDb {
    conn: Mutex<Connection>,
}

impl SuggestionDb {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        let db = Self { conn: Mutex::new(conn) };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS unigrams (
                word TEXT PRIMARY KEY,
                freq INTEGER NOT NULL DEFAULT 1
            );
            CREATE INDEX IF NOT EXISTS idx_unigrams_freq ON unigrams(freq DESC);

            CREATE TABLE IF NOT EXISTS bigrams (
                w1 TEXT NOT NULL,
                w2 TEXT NOT NULL,
                freq INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (w1, w2)
            );
            CREATE INDEX IF NOT EXISTS idx_bigrams_w1 ON bigrams(w1, freq DESC);

            CREATE TABLE IF NOT EXISTS trigrams (
                w1 TEXT NOT NULL,
                w2 TEXT NOT NULL,
                w3 TEXT NOT NULL,
                freq INTEGER NOT NULL DEFAULT 1,
                PRIMARY KEY (w1, w2, w3)
            );
            CREATE INDEX IF NOT EXISTS idx_trigrams_w1w2 ON trigrams(w1, w2, freq DESC);

            -- Keep legacy tables for backward compat during migration
            CREATE TABLE IF NOT EXISTS words (
                word TEXT PRIMARY KEY,
                freq INTEGER NOT NULL DEFAULT 1,
                last_used INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS phrases (
                phrase TEXT PRIMARY KEY,
                freq INTEGER NOT NULL DEFAULT 1,
                last_used INTEGER NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn clear_ngrams(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "DELETE FROM unigrams; DELETE FROM bigrams; DELETE FROM trigrams;",
        )?;
        Ok(())
    }

    pub fn batch_insert_unigrams(&self, counts: &HashMap<String, i64>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO unigrams(word, freq) VALUES (?1, ?2)
                 ON CONFLICT(word) DO UPDATE SET freq = freq + excluded.freq",
            )?;
            for (word, count) in counts {
                stmt.execute(params![word, count])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn batch_insert_bigrams(&self, counts: &HashMap<(String, String), i64>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO bigrams(w1, w2, freq) VALUES (?1, ?2, ?3)
                 ON CONFLICT(w1, w2) DO UPDATE SET freq = freq + excluded.freq",
            )?;
            for ((w1, w2), count) in counts {
                stmt.execute(params![w1, w2, count])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn batch_insert_trigrams(&self, counts: &HashMap<(String, String, String), i64>) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO trigrams(w1, w2, w3, freq) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(w1, w2, w3) DO UPDATE SET freq = freq + excluded.freq",
            )?;
            for ((w1, w2, w3), count) in counts {
                stmt.execute(params![w1, w2, w3, count])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Get next word predictions given two context words (trigram lookup)
    pub fn trigram_predict(&self, w1: &str, w2: &str, prefix: &str, limit: usize) -> Vec<(String, i64)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT w3, freq FROM trigrams
             WHERE w1 = ?1 AND w2 = ?2 AND w3 LIKE (?3 || '%')
             ORDER BY freq DESC LIMIT ?4",
        ).unwrap();
        stmt.query_map(params![w1, w2, prefix, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Get next word predictions given one context word (bigram lookup)
    pub fn bigram_predict(&self, w1: &str, prefix: &str, limit: usize) -> Vec<(String, i64)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT w2, freq FROM bigrams
             WHERE w1 = ?1 AND w2 LIKE (?2 || '%')
             ORDER BY freq DESC LIMIT ?3",
        ).unwrap();
        stmt.query_map(params![w1, prefix, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Get word completions by prefix (unigram lookup)
    pub fn unigram_predict(&self, prefix: &str, limit: usize) -> Vec<(String, i64)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT word, freq FROM unigrams
             WHERE word LIKE (?1 || '%') AND word != ?1
             ORDER BY freq DESC LIMIT ?2",
        ).unwrap();
        stmt.query_map(params![prefix, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }).unwrap().filter_map(|r| r.ok()).collect()
    }

    /// Count entries for stats display
    pub fn count_unigrams(&self) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM unigrams", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn count_bigrams(&self) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM bigrams", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn count_trigrams(&self) -> i64 {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM trigrams", [], |r| r.get(0)).unwrap_or(0)
    }

    /// Load all unigrams into memory
    pub fn load_all_unigrams(&self) -> HashMap<String, i64> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT word, freq FROM unigrams").unwrap();
        let mut map = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }).unwrap();
        for r in rows.flatten() {
            map.insert(r.0, r.1);
        }
        map
    }

    /// Load all bigrams grouped by w1
    pub fn load_all_bigrams(&self) -> HashMap<String, Vec<(String, i64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT w1, w2, freq FROM bigrams ORDER BY w1, freq DESC").unwrap();
        let mut map: HashMap<String, Vec<(String, i64)>> = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?))
        }).unwrap();
        for r in rows.flatten() {
            map.entry(r.0).or_default().push((r.1, r.2));
        }
        // Truncate each to top 10
        for entries in map.values_mut() {
            entries.truncate(10);
        }
        map
    }

    /// Load all trigrams grouped by (w1, w2)
    pub fn load_all_trigrams(&self) -> HashMap<(String, String), Vec<(String, i64)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT w1, w2, w3, freq FROM trigrams ORDER BY w1, w2, freq DESC").unwrap();
        let mut map: HashMap<(String, String), Vec<(String, i64)>> = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?))
        }).unwrap();
        for r in rows.flatten() {
            map.entry((r.0, r.1)).or_default().push((r.2, r.3));
        }
        for entries in map.values_mut() {
            entries.truncate(10);
        }
        map
    }

    #[allow(dead_code)]
    // Legacy methods kept for compatibility
    pub fn add_word(&self, word: &str) -> Result<()> {
        let ts = now_ts();
        self.add_word_with_ts(word, ts, 1)
    }

    pub fn add_phrase(&self, phrase: &str) -> Result<()> {
        let ts = now_ts();
        self.add_phrase_with_ts(phrase, ts, 1)
    }

    pub fn add_word_with_ts(&self, word: &str, ts: i64, inc: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO words(word, freq, last_used) VALUES (?1, ?2, ?3)
               ON CONFLICT(word) DO UPDATE SET freq = freq + excluded.freq, last_used = excluded.last_used"#,
            params![word, inc, ts],
        )?;
        Ok(())
    }

    pub fn add_phrase_with_ts(&self, phrase: &str, ts: i64, inc: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"INSERT INTO phrases(phrase, freq, last_used) VALUES (?1, ?2, ?3)
               ON CONFLICT(phrase) DO UPDATE SET freq = freq + excluded.freq, last_used = excluded.last_used"#,
            params![phrase, inc, ts],
        )?;
        Ok(())
    }

    pub fn best_word_match(&self, prefix: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT word FROM words WHERE word LIKE (?1 || '%') AND word != ?1
             ORDER BY freq DESC, last_used DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![prefix])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    pub fn best_phrase_match(&self, prefix: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT phrase FROM phrases WHERE phrase LIKE (?1 || '%') AND phrase != ?1
             ORDER BY freq DESC, last_used DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![prefix])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
