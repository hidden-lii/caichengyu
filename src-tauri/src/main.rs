// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod entity {
    pub mod board;
    pub mod idiom;
}

mod pinyin;
mod qwen;
mod sqlite;

use entity::board::BoardParseResult;
use entity::idiom::{Idiom, IdiomInput, LexiconMeta, UpsertResult};
use std::path::PathBuf;

fn upsert_err(msg: String) -> UpsertResult {
    UpsertResult {
        added: 0,
        updated: 0,
        skipped: 0,
        errors: vec![msg],
    }
}

/// 在阻塞线程池执行，避免同步命令卡住 UI 主线程。
#[tauri::command]
async fn load_all_idioms() -> Vec<Idiom> {
    tauri::async_runtime::spawn_blocking(|| match sqlite::load_all_idioms() {
        Ok(items) => items,
        Err(e) => {
            eprintln!("load_all_idioms error: {:?}", e);
            vec![]
        }
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
async fn get_lexicon_meta() -> LexiconMeta {
    tauri::async_runtime::spawn_blocking(|| match sqlite::get_lexicon_meta() {
        Ok(meta) => meta,
        Err(e) => {
            eprintln!("get_lexicon_meta error: {:?}", e);
            LexiconMeta {
                total: 0,
                blind_count: 0,
            }
        }
    })
    .await
    .unwrap_or(LexiconMeta {
        total: 0,
        blind_count: 0,
    })
}

#[tauri::command]
async fn add_idiom(item: IdiomInput) -> UpsertResult {
    tauri::async_runtime::spawn_blocking(move || match sqlite::upsert_idioms(vec![item]) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("add_idiom error: {:?}", e);
            upsert_err(format!("{:?}", e))
        }
    })
    .await
    .unwrap_or_else(|e| upsert_err(format!("任务失败: {}", e)))
}

#[tauri::command]
async fn upsert_idioms(items: Vec<IdiomInput>) -> UpsertResult {
    tauri::async_runtime::spawn_blocking(move || match sqlite::upsert_idioms(items) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("upsert_idioms error: {:?}", e);
            upsert_err(format!("{:?}", e))
        }
    })
    .await
    .unwrap_or_else(|e| upsert_err(format!("任务失败: {}", e)))
}

#[tauri::command]
async fn replace_lexicon(items: Vec<IdiomInput>) -> UpsertResult {
    tauri::async_runtime::spawn_blocking(move || match sqlite::replace_lexicon(items) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("replace_lexicon error: {:?}", e);
            upsert_err(format!("{:?}", e))
        }
    })
    .await
    .unwrap_or_else(|e| upsert_err(format!("任务失败: {}", e)))
}

#[tauri::command]
async fn import_lexicon_from_url(url: String) -> UpsertResult {
    tauri::async_runtime::spawn_blocking(move || match sqlite::import_lexicon_from_url(url) {
        Ok(result) => result,
        Err(e) => upsert_err(e),
    })
    .await
    .unwrap_or_else(|e| upsert_err(format!("任务失败: {}", e)))
}

#[tauri::command]
async fn delete_idiom(word: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || match sqlite::delete_idiom(word) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("delete_idiom error: {:?}", e);
            false
        }
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
async fn update_idiom_pinyin(word: String, pinyin: String) -> UpsertResult {
    tauri::async_runtime::spawn_blocking(move || match sqlite::update_idiom_pinyin(word, pinyin)
    {
        Ok(result) => result,
        Err(e) => {
            eprintln!("update_idiom_pinyin error: {:?}", e);
            UpsertResult {
                added: 0,
                updated: 0,
                skipped: 1,
                errors: vec![format!("{:?}", e)],
            }
        }
    })
    .await
    .unwrap_or_else(|e| upsert_err(format!("任务失败: {}", e)))
}

#[tauri::command]
async fn get_setting(key: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || match sqlite::get_setting(key) {
        Ok(value) => value,
        Err(e) => {
            eprintln!("get_setting error: {:?}", e);
            None
        }
    })
    .await
    .unwrap_or(None)
}

#[tauri::command]
async fn set_setting(key: String, value: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || match sqlite::set_setting(key, value) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("set_setting error: {:?}", e);
            false
        }
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
async fn list_qwen_models(api_key: String, key_plan: Option<String>) -> Result<Vec<String>, String> {
    let key_plan = key_plan.unwrap_or_else(|| "dashscope".to_string());
    tauri::async_runtime::spawn_blocking(move || qwen::list_models(&api_key, &key_plan))
        .await
        .map_err(|e| format!("拉取模型任务失败: {}", e))?
}

#[tauri::command]
fn get_qwen_prompt_schema() -> String {
    qwen::PROMPT_SCHEMA_SUFFIX.trim().to_string()
}

#[tauri::command]
fn get_qwen_default_prompt() -> String {
    qwen::default_user_prompt().to_string()
}

/// 在阻塞线程池执行网络请求，避免同步命令卡住 UI 主线程。
/// `stream=true` 时走 SSE，并通过事件 `qwen-ocr-chunk` 推送增量文本。
#[tauri::command]
async fn parse_guess_board(
    window: tauri::Window,
    image_b64: String,
    mime: Option<String>,
    api_key: String,
    key_plan: Option<String>,
    model: String,
    prompt: Option<String>,
    stream: Option<bool>,
) -> Result<BoardParseResult, String> {
    let mime = mime.unwrap_or_else(|| "image/png".to_string());
    let prompt = prompt.unwrap_or_default();
    let stream = stream.unwrap_or(false);
    let key_plan = key_plan.unwrap_or_else(|| "dashscope".to_string());
    tauri::async_runtime::spawn_blocking(move || {
        if stream {
            let mut on_chunk = |delta: &str, text: &str| {
                let _ = window.emit(
                    "qwen-ocr-chunk",
                    serde_json::json!({
                        "delta": delta,
                        "text": text,
                    }),
                );
            };
            qwen::recognize_guess_board_with_progress(
                &image_b64,
                &mime,
                &api_key,
                &key_plan,
                &model,
                &prompt,
                true,
                Some(&mut on_chunk),
            )
        } else {
            qwen::recognize_guess_board(&image_b64, &mime, &api_key, &key_plan, &model, &prompt)
        }
    })
    .await
    .map_err(|e| format!("识别任务失败: {}", e))?
}

fn resolve_seed_path(app: &tauri::App) -> Option<PathBuf> {
    if let Some(path) = app
        .path_resolver()
        .resolve_resource("resources/idiom.json")
    {
        return Some(path);
    }
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/idiom.json");
    if dev_path.exists() {
        return Some(dev_path);
    }
    None
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let app_dir = app
                .path_resolver()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&app_dir).ok();
            let db_path = app_dir.join("caichengyu.db");
            sqlite::initialize_database(db_path).expect("数据库初始化失败");

            if let Some(seed_path) = resolve_seed_path(app) {
                match sqlite::seed_if_empty(&seed_path) {
                    Ok(seeded) if seeded => eprintln!("seeded lexicon from {:?}", seed_path),
                    Err(e) => eprintln!("seed error: {:?}", e),
                    _ => {}
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_all_idioms,
            get_lexicon_meta,
            add_idiom,
            upsert_idioms,
            replace_lexicon,
            import_lexicon_from_url,
            delete_idiom,
            update_idiom_pinyin,
            get_setting,
            set_setting,
            list_qwen_models,
            get_qwen_prompt_schema,
            get_qwen_default_prompt,
            parse_guess_board
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
