use rusqlite::{params, Connection, Result};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct SuggestionDb {
    conn: Connection,
}

impl SuggestionDb {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
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

            CREATE INDEX IF NOT EXISTS idx_words_freq
            ON words(freq DESC, last_used DESC);

            CREATE INDEX IF NOT EXISTS idx_phrases_freq
            ON phrases(freq DESC, last_used DESC);
            "#,
        )?;
        Ok(())
    }

    pub fn add_word(&self, word: &str) -> Result<()> {
        let ts = now_ts();
        self.add_word_with_ts(word, ts, 1)
    }

    pub fn add_phrase(&self, phrase: &str) -> Result<()> {
        let ts = now_ts();
        self.add_phrase_with_ts(phrase, ts, 1)
    }

    pub fn add_word_with_ts(&self, word: &str, ts: i64, inc: i64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO words(word, freq, last_used)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(word) DO UPDATE SET
                freq = freq + excluded.freq,
                last_used = excluded.last_used
            "#,
            params![word, inc, ts],
        )?;
        Ok(())
    }

    pub fn add_phrase_with_ts(&self, phrase: &str, ts: i64, inc: i64) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO phrases(phrase, freq, last_used)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(phrase) DO UPDATE SET
                freq = freq + excluded.freq,
                last_used = excluded.last_used
            "#,
            params![phrase, inc, ts],
        )?;
        Ok(())
    }

    pub fn best_word_match(&self, prefix: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT word
            FROM words
            WHERE word LIKE (?1 || '%')
              AND word != ?1
            ORDER BY freq DESC, last_used DESC
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query(params![prefix])?;
        if let Some(row) = rows.next()? {
            let word: String = row.get(0)?;
            Ok(Some(word))
        } else {
            Ok(None)
        }
    }

    pub fn best_phrase_match(&self, prefix: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT phrase
            FROM phrases
            WHERE phrase LIKE (?1 || '%')
              AND phrase != ?1
            ORDER BY freq DESC, last_used DESC
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query(params![prefix])?;
        if let Some(row) = rows.next()? {
            let phrase: String = row.get(0)?;
            Ok(Some(phrase))
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