use crate::entity::idiom::{Idiom, IdiomInput, LexiconMeta, UpsertResult};
use crate::pinyin::{build_chars, is_blind_eligible, normalize_entry};
use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref DB: Mutex<Option<Connection>> = Mutex::new(None);
}

fn with_conn<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    let guard = DB.lock().unwrap();
    let conn = guard.as_ref().expect("database not initialized");
    f(conn)
}

pub fn initialize_database(db_path: PathBuf) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(&db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS idiom (
            id INTEGER PRIMARY KEY,
            word TEXT NOT NULL UNIQUE,
            pinyin TEXT NOT NULL,
            explanation TEXT NOT NULL DEFAULT '',
            char_count INTEGER NOT NULL,
            chars_json TEXT NOT NULL,
            blind_eligible INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS app_setting (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_idiom_blind ON idiom(blind_eligible);
        CREATE INDEX IF NOT EXISTS idx_idiom_len ON idiom(char_count);",
    )?;
    *DB.lock().unwrap() = Some(conn);
    Ok(())
}

pub fn seed_if_empty(seed_path: &Path) -> Result<bool> {
    let count: i64 = with_conn(|conn| {
        conn.query_row("SELECT COUNT(*) FROM idiom", [], |row| row.get(0))
    })?;
    if count > 0 {
        return Ok(false);
    }
    if !seed_path.exists() {
        return Ok(false);
    }
    let raw = fs::read_to_string(seed_path)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    let items: Vec<IdiomInput> = serde_json::from_str(&raw)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    upsert_idioms_internal(&items, false)?;
    Ok(true)
}

fn row_to_idiom(
    id: i64,
    word: String,
    pinyin: String,
    explanation: String,
    char_count: i32,
    chars_json: String,
    blind_eligible: i32,
) -> rusqlite::Result<Idiom> {
    let chars: Vec<crate::entity::idiom::CharData> = serde_json::from_str(&chars_json)
        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
    Ok(Idiom {
        id: Some(id),
        word,
        pinyin,
        explanation,
        char_count,
        chars,
        blind_eligible: blind_eligible != 0,
    })
}

pub fn load_all_idioms() -> Result<Vec<Idiom>> {
    with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, word, pinyin, explanation, char_count, chars_json, blind_eligible FROM idiom ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            row_to_idiom(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            )
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    })
}

pub fn get_lexicon_meta() -> Result<LexiconMeta> {
    with_conn(|conn| {
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM idiom", [], |row| row.get(0))?;
        let blind_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM idiom WHERE blind_eligible = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(LexiconMeta { total, blind_count })
    })
}

fn upsert_idioms_internal(items: &[IdiomInput], replace: bool) -> Result<UpsertResult> {
    with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        if replace {
            tx.execute("DELETE FROM idiom", [])?;
        }

        let mut added = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();

        for item in items {
            let Some(entry) = normalize_entry(item) else {
                skipped += 1;
                errors.push("存在无效条目（缺少 word 或 pinyin）".to_string());
                continue;
            };
            let Some(chars) = build_chars(&entry.word, &entry.pinyin) else {
                skipped += 1;
                errors.push(format!(
                    "「{}」拼音音节数与字数不一致",
                    entry.word
                ));
                continue;
            };
            let blind = is_blind_eligible(&chars);
            let chars_json = serde_json::to_string(&chars)
                .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            let explanation = entry.explanation.clone().unwrap_or_default();
            let char_count = chars.len() as i32;

            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM idiom WHERE word = ?1",
                    params![entry.word],
                    |row| row.get(0),
                )
                .ok();

            if let Some(id) = existing {
                tx.execute(
                    "UPDATE idiom SET pinyin = ?1, explanation = ?2, char_count = ?3, chars_json = ?4, blind_eligible = ?5 WHERE id = ?6",
                    params![entry.pinyin, explanation, char_count, chars_json, blind as i32, id],
                )?;
                updated += 1;
            } else {
                tx.execute(
                    "INSERT INTO idiom (word, pinyin, explanation, char_count, chars_json, blind_eligible) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![entry.word, entry.pinyin, explanation, char_count, chars_json, blind as i32],
                )?;
                added += 1;
            }
        }

        tx.commit()?;
        Ok(UpsertResult {
            added,
            updated,
            skipped,
            errors,
        })
    })
}

pub fn upsert_idioms(items: Vec<IdiomInput>) -> Result<UpsertResult> {
    upsert_idioms_internal(&items, false)
}

pub fn replace_lexicon(items: Vec<IdiomInput>) -> Result<UpsertResult> {
    upsert_idioms_internal(&items, true)
}

pub fn delete_idiom(word: String) -> Result<bool> {
    with_conn(|conn| {
        let n = conn.execute("DELETE FROM idiom WHERE word = ?1", params![word])?;
        Ok(n > 0)
    })
}

/// 仅更新已存在成语的拼音（保留释义），接受音标或数字调号。
pub fn update_idiom_pinyin(word: String, pinyin: String) -> Result<UpsertResult> {
    let word = word.trim().to_string();
    let pinyin = crate::pinyin::normalize_pinyin_input(&pinyin);
    if word.is_empty() || pinyin.is_empty() {
        return Ok(UpsertResult {
            added: 0,
            updated: 0,
            skipped: 1,
            errors: vec!["成语或拼音不能为空".to_string()],
        });
    }

    with_conn(|conn| {
        let existing: Option<(i64, String)> = conn
            .query_row(
                "SELECT id, explanation FROM idiom WHERE word = ?1",
                params![word],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        let Some((id, _explanation)) = existing else {
            return Ok(UpsertResult {
                added: 0,
                updated: 0,
                skipped: 1,
                errors: vec![format!("词库中不存在「{}」", word)],
            });
        };

        let Some(chars) = build_chars(&word, &pinyin) else {
            return Ok(UpsertResult {
                added: 0,
                updated: 0,
                skipped: 1,
                errors: vec![format!("「{}」拼音音节数与字数不一致", word)],
            });
        };

        let blind = is_blind_eligible(&chars);
        let chars_json = serde_json::to_string(&chars)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let char_count = chars.len() as i32;

        conn.execute(
            "UPDATE idiom SET pinyin = ?1, char_count = ?2, chars_json = ?3, blind_eligible = ?4 WHERE id = ?5",
            params![pinyin, char_count, chars_json, blind as i32, id],
        )?;

        Ok(UpsertResult {
            added: 0,
            updated: 1,
            skipped: 0,
            errors: vec![],
        })
    })
}

pub fn import_lexicon_from_url(url: String) -> std::result::Result<UpsertResult, String> {
    let resp = reqwest::blocking::get(&url).map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let data: Vec<serde_json::Value> = resp.json().map_err(|e| e.to_string())?;
    let items: Vec<IdiomInput> = data
        .into_iter()
        .filter_map(|v| {
            Some(IdiomInput {
                word: v.get("word")?.as_str()?.to_string(),
                pinyin: v.get("pinyin")?.as_str()?.to_string(),
                explanation: v
                    .get("explanation")
                    .and_then(|e| e.as_str())
                    .map(|s| s.to_string()),
            })
        })
        .collect();
    replace_lexicon(items).map_err(|e| e.to_string())
}

pub fn get_setting(key: String) -> Result<Option<String>> {
    with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT value FROM app_setting WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    })
}

pub fn set_setting(key: String, value: String) -> Result<()> {
    with_conn(|conn| {
        conn.execute(
            "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    })
}
