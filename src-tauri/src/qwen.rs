use crate::entity::board::{BoardParseResult, ParsedGuess, PositionMarksRaw};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// 百炼按量付费（通用 API Key，`sk-` 开头）
const BASE_URL_DASHSCOPE: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
/// Token Plan 专属（`sk-sp-` 开头）
const BASE_URL_TOKEN_PLAN: &str =
    "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1";
/// Coding Plan 专属（`sk-sp-` 开头）
const BASE_URL_CODING_PLAN: &str = "https://coding.dashscope.aliyuncs.com/v1";

/// Key 类型：与 Base URL 配套，不可混用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyPlan {
    DashScope,
    TokenPlan,
    CodingPlan,
}

impl KeyPlan {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "token_plan" | "token-plan" | "tokenplan" => Self::TokenPlan,
            "coding_plan" | "coding-plan" | "codingplan" => Self::CodingPlan,
            _ => Self::DashScope,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DashScope => "dashscope",
            Self::TokenPlan => "token_plan",
            Self::CodingPlan => "coding_plan",
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Self::DashScope => BASE_URL_DASHSCOPE,
            Self::TokenPlan => BASE_URL_TOKEN_PLAN,
            Self::CodingPlan => BASE_URL_CODING_PLAN,
        }
    }

    pub fn fallback_models(self) -> &'static [&'static str] {
        match self {
            Self::DashScope => FALLBACK_MODELS_DASHSCOPE,
            Self::TokenPlan => FALLBACK_MODELS_TOKEN_PLAN,
            Self::CodingPlan => FALLBACK_MODELS_CODING_PLAN,
        }
    }
}

/// 无论用户如何改写自定义 prompt，后端都会强制追加这段输出约束。
pub const PROMPT_SCHEMA_SUFFIX: &str = r#"

【输出硬性要求——不可违反，优先级高于上文任何说明】
你必须只输出一个合法 JSON 对象（不要用 markdown 代码块包裹，不要输出解释文字），格式严格如下：
{
  "guesses": [
    {
      "word": "四字成语",
      "marks": [
        { "char": "hit|present|absent", "sm": "hit|present|absent", "ym": "hit|present|absent", "tone": "hit|present|absent" },
        { "char": "hit|present|absent", "sm": "hit|present|absent", "ym": "hit|present|absent", "tone": "hit|present|absent" },
        { "char": "hit|present|absent", "sm": "hit|present|absent", "ym": "hit|present|absent", "tone": "hit|present|absent" },
        { "char": "hit|present|absent", "sm": "hit|present|absent", "ym": "hit|present|absent", "tone": "hit|present|absent" }
      ]
    }
  ],
  "warnings": []
}
规则：
1. guesses 按截图从上到下排列每一次猜测
2. 每个 guess.marks 长度必须为 4，依次对应成语第 1～4 字
3. char / sm / ym / tone 的取值只能是 "hit"、"present"、"absent" 三者之一
4. 颜色约定：绿色=hit，紫色或粉色=present，灰色=absent；无法确定时用 absent，并写入 warnings
5. 除上述 JSON 外不得输出任何其他字符
"#;

const FALLBACK_MODELS_DASHSCOPE: &[&str] = &[
    "qwen3-vl-plus",
    "qwen3-vl-flash",
    "qwen-vl-max",
    "qwen-vl-plus",
    "qwen-vl-ocr-latest",
    "qwen2.5-vl-72b-instruct",
    "qwen2.5-vl-32b-instruct",
    "qwen2.5-vl-7b-instruct",
];

/// Token Plan 套餐内具备视觉理解能力的常用模型
const FALLBACK_MODELS_TOKEN_PLAN: &[&str] = &[
    "qwen3.7-plus",
    "qwen3.6-plus",
    "qwen3.6-flash",
    "qwen3.8-max",
    "kimi-k2.5",
    "kimi-k2.6",
    "kimi-k2.7-code",
];

/// Coding Plan 套餐内支持图片理解的模型
const FALLBACK_MODELS_CODING_PLAN: &[&str] = &[
    "qwen3.7-plus",
    "qwen3.6-plus",
    "qwen3.5-plus",
    "kimi-k2.5",
];

fn http_client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Option<Vec<ModelItem>>,
}

#[derive(Debug, Deserialize)]
struct ModelItem {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Option<Vec<ChatChoice>>,
    error: Option<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: Option<String>,
    code: Option<String>,
}

pub fn list_models(api_key: &str, key_plan: &str) -> Result<Vec<String>, String> {
    let key = api_key.trim();
    if key.is_empty() {
        return Err("请先填写 API Key".to_string());
    }

    let plan = KeyPlan::parse(key_plan);
    let base = plan.base_url();
    let client = http_client()?;
    let resp = client
        .get(format!("{}/models", base))
        .bearer_auth(key)
        .send()
        .map_err(|e| format!("请求模型列表失败: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .map_err(|e| format!("读取模型列表响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!(
            "模型列表请求失败 ({}) [{}]: {}",
            status.as_u16(),
            plan.as_str(),
            truncate(&text, 300)
        ));
    }

    let parsed: ModelsResponse = serde_json::from_str(&text)
        .map_err(|e| format!("解析模型列表失败: {}；响应: {}", e, truncate(&text, 200)))?;

    let mut ids: Vec<String> = parsed
        .data
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.id)
        .filter(|id| !id.is_empty())
        .collect();

    ids.sort();
    ids.dedup();

    if ids.is_empty() {
        return Ok(plan
            .fallback_models()
            .iter()
            .map(|s| (*s).to_string())
            .collect());
    }
    Ok(ids)
}

pub fn build_full_prompt(user_prompt: &str) -> String {
    let custom = user_prompt.trim();
    if custom.is_empty() {
        format!("{}{}", default_user_prompt(), PROMPT_SCHEMA_SUFFIX)
    } else {
        format!("{}{}", custom, PROMPT_SCHEMA_SUFFIX)
    }
}

pub fn default_user_prompt() -> &'static str {
    r#"你是「猜成语」游戏截图解析助手。图片是游戏界面截图，包含若干次猜测。
每次猜测是一个四字成语；每个字有四个维度的颜色标记：字、声（声母）、韵（韵母）、调（声调）。
颜色含义：绿色=正确且位置对(hit)，紫色/粉色=存在但位置不对(present)，灰色=不存在(absent)。
请识别每一行猜测的成语文字，以及每个字四个维度的标记状态。"#
}

/// `on_chunk(delta, accumulated)`：流式模式下每收到一段文本回调一次。
pub fn recognize_guess_board(
    image_b64: &str,
    mime: &str,
    api_key: &str,
    key_plan: &str,
    model: &str,
    user_prompt: &str,
) -> Result<BoardParseResult, String> {
    recognize_guess_board_with_progress(
        image_b64,
        mime,
        api_key,
        key_plan,
        model,
        user_prompt,
        false,
        None,
    )
}

pub fn recognize_guess_board_with_progress(
    image_b64: &str,
    mime: &str,
    api_key: &str,
    key_plan: &str,
    model: &str,
    user_prompt: &str,
    stream: bool,
    mut on_chunk: Option<&mut dyn FnMut(&str, &str)>,
) -> Result<BoardParseResult, String> {
    let key = api_key.trim();
    let model = model.trim();
    if key.is_empty() {
        return Err("请先填写千问 API Key".to_string());
    }
    if model.is_empty() {
        return Err("请选择模型".to_string());
    }

    let b64 = image_b64.trim();
    if b64.is_empty() {
        return Err("图片数据为空".to_string());
    }

    let plan = KeyPlan::parse(key_plan);
    let mime = normalize_mime(mime);
    let prompt = build_full_prompt(user_prompt);
    let data_url = format!("data:{};base64,{}", mime, b64);

    let body = json!({
        "model": model,
        "stream": stream,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "image_url",
                    "image_url": { "url": data_url }
                },
                {
                    "type": "text",
                    "text": prompt
                }
            ]
        }]
    });

    let client = http_client()?;
    let resp = client
        .post(format!("{}/chat/completions", plan.base_url()))
        .bearer_auth(key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .map_err(|e| format!("调用千问失败: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp
            .text()
            .map_err(|e| format!("读取千问响应失败: {}", e))?;
        return Err(format!(
            "千问请求失败 ({}): {}",
            status.as_u16(),
            truncate(&text, 400)
        ));
    }

    let content_text = if stream {
        read_sse_content(resp, &mut on_chunk)?
    } else {
        let text = resp
            .text()
            .map_err(|e| format!("读取千问响应失败: {}", e))?;
        parse_non_stream_content(&text)?
    };

    if content_text.trim().is_empty() {
        return Err("千问未返回内容".to_string());
    }

    let mut parsed = parse_board_json(&content_text)?;
    parsed.raw_response = content_text;
    Ok(parsed)
}

fn parse_non_stream_content(text: &str) -> Result<String, String> {
    let chat: ChatResponse = serde_json::from_str(text)
        .map_err(|e| format!("解析千问响应失败: {}；响应: {}", e, truncate(text, 300)))?;

    if let Some(err) = chat.error {
        return Err(format!(
            "千问返回错误: {}",
            err.message
                .unwrap_or_else(|| err.code.unwrap_or_else(|| "unknown".into()))
        ));
    }

    let content = chat
        .choices
        .and_then(|mut c| c.pop())
        .and_then(|c| c.message.content)
        .ok_or_else(|| "千问未返回内容".to_string())?;

    content_to_text(&content)
}

fn read_sse_content(
    resp: reqwest::blocking::Response,
    on_chunk: &mut Option<&mut dyn FnMut(&str, &str)>,
) -> Result<String, String> {
    let reader = BufReader::new(resp);
    let mut full = String::new();

    for line_res in reader.lines() {
        let line = line_res.map_err(|e| format!("读取流式响应失败: {}", e))?;
        if line.is_empty() {
            continue;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            break;
        }

        let value: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(err) = value.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .or_else(|| err.get("code").and_then(|c| c.as_str()))
                .unwrap_or("unknown");
            return Err(format!("千问返回错误: {}", msg));
        }

        let delta = extract_stream_delta(&value);
        if delta.is_empty() {
            continue;
        }
        full.push_str(&delta);
        if let Some(cb) = on_chunk.as_mut() {
            cb(&delta, &full);
        }
    }

    Ok(full)
}

fn extract_stream_delta(value: &Value) -> String {
    let Some(choices) = value.get("choices").and_then(|c| c.as_array()) else {
        return String::new();
    };
    let Some(first) = choices.first() else {
        return String::new();
    };

    // OpenAI 兼容：choices[0].delta.content
    if let Some(delta) = first.get("delta") {
        if let Some(s) = delta.get("content").and_then(|c| c.as_str()) {
            return s.to_string();
        }
        // 部分多模态：content 为数组
        if let Some(arr) = delta.get("content").and_then(|c| c.as_array()) {
            return arr
                .iter()
                .filter_map(|p| {
                    p.get("text")
                        .and_then(|t| t.as_str())
                        .or_else(|| p.as_str())
                })
                .collect::<Vec<_>>()
                .join("");
        }
    }

    // 兜底：message.content
    if let Some(s) = first
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
    {
        return s.to_string();
    }

    String::new()
}

fn content_to_text(content: &Value) -> Result<String, String> {
    match content {
        Value::String(s) => Ok(s.clone()),
        Value::Array(parts) => {
            let mut texts = Vec::new();
            for part in parts {
                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                    texts.push(t.to_string());
                } else if let Some(t) = part.as_str() {
                    texts.push(t.to_string());
                }
            }
            if texts.is_empty() {
                Err("千问返回内容为空".to_string())
            } else {
                Ok(texts.join("\n"))
            }
        }
        other => Ok(other.to_string()),
    }
}

fn parse_board_json(raw: &str) -> Result<BoardParseResult, String> {
    let json_str = extract_json_object(raw)?;
    let value: Value = serde_json::from_str(&json_str)
        .map_err(|e| format!("模型输出不是合法 JSON: {}；原文: {}", e, truncate(raw, 300)))?;

    let guesses_val = value
        .get("guesses")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "JSON 缺少 guesses 数组".to_string())?;

    let mut guesses = Vec::new();
    let mut warnings: Vec<String> = value
        .get("warnings")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|w| w.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    for (i, g) in guesses_val.iter().enumerate() {
        match normalize_guess(g) {
            Ok(guess) => guesses.push(guess),
            Err(e) => warnings.push(format!("第 {} 条猜测解析失败：{}", i + 1, e)),
        }
    }

    Ok(BoardParseResult {
        guesses,
        warnings,
        raw_response: String::new(),
    })
}

fn normalize_guess(value: &Value) -> Result<ParsedGuess, String> {
    let word = value
        .get("word")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();

    if word.chars().count() != 4 {
        return Err(format!("成语「{}」不是四字", word));
    }

    let marks_arr = value
        .get("marks")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "缺少 marks".to_string())?;

    let mut marks = Vec::with_capacity(4);
    for i in 0..4 {
        let m = marks_arr.get(i).unwrap_or(&Value::Null);
        marks.push(PositionMarksRaw {
            char: normalize_mark(m.get("char")),
            sm: normalize_mark(m.get("sm")),
            ym: normalize_mark(m.get("ym")),
            tone: normalize_mark(m.get("tone")),
        });
    }

    Ok(ParsedGuess {
        word,
        marks,
        cells: vec![],
    })
}

fn normalize_mark(v: Option<&Value>) -> String {
    let raw = v
        .and_then(|x| x.as_str())
        .unwrap_or("absent")
        .trim()
        .to_lowercase();
    match raw.as_str() {
        "hit" | "correct" | "green" | "对" | "正确" => "hit".to_string(),
        "present" | "purple" | "pink" | "偏" | "存在" => "present".to_string(),
        _ => "absent".to_string(),
    }
}

fn extract_json_object(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.starts_with('{') {
        return Ok(trimmed.to_string());
    }

    // ```json ... ```
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if end > start {
                return Ok(trimmed[start..=end].to_string());
            }
        }
    }

    Err(format!("未找到 JSON 对象：{}", truncate(raw, 200)))
}

fn normalize_mime(mime: &str) -> &str {
    let m = mime.trim().to_lowercase();
    match m.as_str() {
        "image/jpeg" | "image/jpg" | "jpg" | "jpeg" => "image/jpeg",
        "image/webp" | "webp" => "image/webp",
        "image/gif" | "gif" => "image/gif",
        _ => "image/png",
    }
}

fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        format!("{}…", chars[..max].iter().collect::<String>())
    }
}
