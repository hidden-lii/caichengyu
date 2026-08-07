use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharData {
    pub c: String,
    pub py: String,
    pub sm: String,
    pub ym: String,
    pub tone: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Idiom {
    pub id: Option<i64>,
    pub word: String,
    pub pinyin: String,
    pub explanation: String,
    pub char_count: i32,
    pub chars: Vec<CharData>,
    pub blind_eligible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdiomInput {
    pub word: String,
    pub pinyin: String,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LexiconMeta {
    pub total: i64,
    pub blind_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertResult {
    pub added: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinLexiconInfo {
    pub id: String,
    pub name: String,
    pub description: String,
}
