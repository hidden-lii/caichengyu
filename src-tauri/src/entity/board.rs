use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionMarksRaw {
    pub char: String,
    pub sm: String,
    pub ym: String,
    pub tone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCell {
    pub col: usize,
    pub attr: String,
    pub text: String,
    pub rgb: Vec<u8>,
    pub mark: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedGuess {
    pub word: String,
    pub marks: Vec<PositionMarksRaw>,
    #[serde(default)]
    pub cells: Vec<ParsedCell>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardParseResult {
    pub guesses: Vec<ParsedGuess>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// 模型原始返回文本（便于排查与历史记录）
    #[serde(default)]
    pub raw_response: String,
}
