//! Local OCR: PP-OCRv5 (paddle-ocr-rs).
//!
//! Only runs detection once on the preprocessed full image (instead of once per color channel, 4x).
//! Color marks are derived by voting over the color masks within each detected box's region
//! (see [`preprocess::vote_mark`]) rather than single-pixel sampling.
//! Initial/final/tone sub-regions are split proportionally using
//! [`crate::pinyin::parse_syllable`]'s existing initial-consonant table.

use crate::entity::board::{BoardParseResult, ParsedGuess, PositionMarksRaw};
use crate::entity::idiom::Idiom;
use crate::preprocess::{self, ColorMark, PreprocessedBoard};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use paddle_ocr_rs::ocr_lite::OcrLite;
use paddle_ocr_rs::ocr_result::TextBlock;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const DET_NAME: &str = "ch_PP-OCRv5_mobile_det.onnx";
const CLS_NAME: &str = "ch_ppocr_mobile_v2.0_cls_infer.onnx";
const REC_NAME: &str = "ch_PP-OCRv5_rec_mobile_infer.onnx";

// Upper bound for the detector network's input side length. Before this fix it was
// effectively equal to the whole (upscaled) image's own side length, so the detector
// ran a forward pass over the full-resolution image with no real downscaling at all -
// that was the main performance bug. Recognition (CRNN) crops come from the upscaled
// source image regardless of this value, so capping it barely affects sharpness.
const MAX_DET_SIDE: u32 = 1536;

static OCR_ENGINE: Mutex<Option<OcrLite>> = Mutex::new(None);
static MODEL_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Set model directory at app startup (resource or dev path).
pub fn set_model_dir(dir: PathBuf) {
    if let Ok(mut guard) = MODEL_DIR.lock() {
        *guard = Some(dir);
    }
}

fn resolve_model_dir() -> Result<PathBuf, String> {
    if let Ok(guard) = MODEL_DIR.lock() {
        if let Some(ref dir) = *guard {
            if dir.join(DET_NAME).exists() {
                return Ok(dir.clone());
            }
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("ocr");
    if dev.join(DET_NAME).exists() {
        return Ok(dev);
    }
    Err(format!(
        "OCR models not found (need {DET_NAME} / {CLS_NAME} / {REC_NAME}). Check resources/ocr."
    ))
}

fn ensure_engine() -> Result<(), String> {
    let mut guard = OCR_ENGINE
        .lock()
        .map_err(|_| "OCR engine lock poisoned".to_string())?;
    if guard.is_some() {
        return Ok(());
    }
    let dir = resolve_model_dir()?;
    let det = dir.join(DET_NAME);
    let cls = dir.join(CLS_NAME);
    let rec = dir.join(REC_NAME);
    for p in [&det, &cls, &rec] {
        if !p.exists() {
            return Err(format!("missing model file: {}", p.display()));
        }
    }
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .min(8);
    let mut ocr = OcrLite::new();
    ocr.init_models(path_str(&det)?, path_str(&cls)?, path_str(&rec)?, threads)
        .map_err(|e| format!("failed to load OCR models: {e}"))?;
    *guard = Some(ocr);
    Ok(())
}

fn path_str(p: &Path) -> Result<&str, String> {
    p.to_str()
        .ok_or_else(|| format!("invalid model path: {}", p.display()))
}

#[derive(Debug, Clone)]
struct DetectedItem {
    text: String,
    score: f32,
    cx: f32,
    cy: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    // Region-vote color mark over this box's own area; this is the "char" mark for CJK boxes.
    mark: ColorMark,
    // Whether the recognized text contains CJK (character box vs pinyin/tone box).
    is_cjk: bool,
}

/// Recognize a guess-board screenshot.
pub fn recognize_guess_board(image_b64: &str, scale: f32) -> Result<BoardParseResult, String> {
    ensure_engine()?;
    let bytes = decode_image_b64(image_b64)?;
    let prepared = preprocess::preprocess_image(&bytes, scale)?;
    let items = detect_once(&prepared)?;
    let idioms = if crate::sqlite::is_initialized() {
        crate::sqlite::load_all_idioms().unwrap_or_default()
    } else {
        Vec::new()
    };
    Ok(assemble_board(&prepared, &items, &idioms))
}

fn decode_image_b64(image_b64: &str) -> Result<Vec<u8>, String> {
    let raw = image_b64.trim();
    let raw = raw
        .strip_prefix("data:")
        .and_then(|s| s.split(',').nth(1))
        .unwrap_or(raw);
    B64.decode(raw.as_bytes())
        .map_err(|e| format!("base64 decode failed: {e}"))
}

// Single detection pass over the whole preprocessed image (previously: one pass per
// full/hit/present/absent channel, four passes total).
fn detect_once(prepared: &PreprocessedBoard) -> Result<Vec<DetectedItem>, String> {
    let mut guard = OCR_ENGINE
        .lock()
        .map_err(|_| "OCR engine lock poisoned".to_string())?;
    let ocr = guard
        .as_mut()
        .ok_or_else(|| "OCR engine not initialized".to_string())?;

    let result = ocr
        .detect(
            &prepared.ocr_bin,
            50,
            MAX_DET_SIDE,
            0.5,
            0.3,
            1.6,
            false,
            false,
        )
        .map_err(|e| format!("OCR detect failed: {e}"))?;

    let mut out = Vec::new();
    for block in &result.text_blocks {
        if let Some(item) = text_block_to_item(block, prepared) {
            out.push(item);
        }
    }
    Ok(out)
}

fn text_block_to_item(block: &TextBlock, prepared: &PreprocessedBoard) -> Option<DetectedItem> {
    let text = block.text.trim().to_string();
    if text.is_empty() {
        return None;
    }
    let is_cjk = contains_cjk(&text);
    // Pinyin/tone boxes are only used for locating position and sampling color, not for
    // exact text recognition, so their confidence bar can be much lower than CJK boxes.
    let min_score = if is_cjk { 0.35 } else { 0.10 };
    if block.text_score < min_score {
        return None;
    }
    let (x0, y0, x1, y1, cx, cy) = box_bounds(&block.box_points)?;
    let mark = preprocess::vote_mark(
        &prepared.hit_mask,
        &prepared.present_mask,
        prepared.upscaled.width(),
        prepared.upscaled.height(),
        x0,
        y0,
        x1,
        y1,
    );
    Some(DetectedItem {
        text,
        score: block.text_score,
        cx,
        cy,
        x0,
        y0,
        x1,
        y1,
        mark,
        is_cjk,
    })
}

fn box_bounds(
    points: &[paddle_ocr_rs::ocr_result::Point],
) -> Option<(f32, f32, f32, f32, f32, f32)> {
    if points.is_empty() {
        return None;
    }
    let mut x0 = f32::MAX;
    let mut y0 = f32::MAX;
    let mut x1 = f32::MIN;
    let mut y1 = f32::MIN;
    for p in points {
        let x = p.x as f32;
        let y = p.y as f32;
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    Some((x0, y0, x1, y1, (x0 + x1) * 0.5, (y0 + y1) * 0.5))
}

fn assemble_board(
    prepared: &PreprocessedBoard,
    items: &[DetectedItem],
    idioms: &[Idiom],
) -> BoardParseResult {
    let mask_w = prepared.upscaled.width();
    let mask_h = prepared.upscaled.height();

    let mut warnings = Vec::new();
    let mut raw_lines = Vec::new();
    raw_lines.push(format!("local-paddle-ocr scale={:.2}", prepared.scale));
    for it in items {
        raw_lines.push(format!(
            "{:.2} ({:.0},{:.0}) {} {:?}",
            it.score, it.cx, it.cy, it.text, it.mark
        ));
    }

    let pinyin_items: Vec<&DetectedItem> = items.iter().filter(|it| !it.is_cjk).collect();

    let mut char_items: Vec<DetectedItem> = items
        .iter()
        .filter(|it| is_likely_idiom_chars(&it.text))
        .cloned()
        .collect();

    if char_items.is_empty() {
        warnings.push(WARN_NO_CHARS.to_string());
        return BoardParseResult {
            guesses: vec![],
            warnings,
            raw_response: raw_lines.join("\n"),
        };
    }

    char_items = dedupe_items(char_items);
    let rows = cluster_rows(&char_items);
    let mut guesses = Vec::new();

    for row in rows {
        let mut cells = row;
        cells.sort_by(|a, b| a.cx.partial_cmp(&b.cx).unwrap_or(std::cmp::Ordering::Equal));

        // (text, mark, cx, cy, height) - height carries through so pronunciation-mark
        // search radii can scale with the actual on-screen character size instead of
        // fixed pixel constants (which broke badly once the image is upscaled).
        let mut chars: Vec<(String, ColorMark, f32, f32, f32)> = Vec::new();
        for cell in &cells {
            let cleaned = keep_cjk(&cell.text);
            if cleaned.is_empty() {
                continue;
            }
            let height = (cell.y1 - cell.y0).max(8.0);
            if cleaned.chars().count() == 1 {
                chars.push((cleaned, cell.mark, cell.cx, cell.cy, height));
            } else {
                let n = cleaned.chars().count().max(1) as f32;
                let width = (cell.x1 - cell.x0).max(1.0);
                for (i, ch) in cleaned.chars().enumerate() {
                    let t0 = i as f32 / n;
                    let t1 = (i as f32 + 1.0) / n;
                    let sub_x0 = cell.x0 + width * t0;
                    let sub_x1 = cell.x0 + width * t1;
                    let cx = (sub_x0 + sub_x1) * 0.5;
                    let mark = preprocess::vote_mark(
                        &prepared.hit_mask,
                        &prepared.present_mask,
                        mask_w,
                        mask_h,
                        sub_x0,
                        cell.y0,
                        sub_x1,
                        cell.y1,
                    );
                    chars.push((ch.to_string(), mark, cx, cell.cy, height));
                }
            }
        }

        if chars.is_empty() {
            continue;
        }

        if chars.len() > 4 {
            let text: String = chars.iter().map(|(c, _, _, _, _)| c.as_str()).collect();
            warnings.push(format!(
                "{}{}{}{}",
                WARN_TOO_MANY_A,
                chars.len(),
                WARN_TOO_MANY_B,
                text
            ));
            chars.truncate(4);
        }
        if chars.len() < 4 {
            let text: String = chars.iter().map(|(c, _, _, _, _)| c.as_str()).collect();
            warnings.push(format!(
                "{}{}{}{}",
                WARN_TOO_FEW_A,
                chars.len(),
                WARN_TOO_FEW_B,
                text
            ));
            while chars.len() < 4 {
                chars.push(("?".into(), ColorMark::Absent, 0.0, 0.0, 40.0));
            }
        }

        let word: String = chars.iter().map(|(c, _, _, _, _)| c.as_str()).collect();
        let marks: Vec<PositionMarksRaw> = chars
            .iter()
            .map(|(_, mark, cx, cy, height)| {
                let (sm, ym, tone) =
                    sample_pronunciation_marks(prepared, *cx, *cy, *height, &pinyin_items);
                PositionMarksRaw {
                    char: mark.as_str().to_string(),
                    sm: sm.as_str().to_string(),
                    ym: ym.as_str().to_string(),
                    tone: tone.as_str().to_string(),
                }
            })
            .collect();

        let (final_word, correction_note) = correct_with_lexicon(&word, idioms);
        if let Some(note) = correction_note {
            warnings.push(note);
        }

        guesses.push(ParsedGuess {
            word: final_word,
            marks,
            cells: vec![],
        });
    }

    if guesses.is_empty() {
        warnings.push(WARN_NO_GUESSES.to_string());
    }

    BoardParseResult {
        guesses,
        warnings,
        raw_response: raw_lines.join("\n"),
    }
}

fn is_cjk_char(c: char) -> bool {
    let u = c as u32;
    (0x4E00..=0x9FFF).contains(&u) || (0x3400..=0x4DBF).contains(&u)
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(is_cjk_char)
}

fn is_likely_idiom_chars(text: &str) -> bool {
    let cjk = keep_cjk(text);
    !cjk.is_empty() && cjk.chars().count() <= 8
}

fn keep_cjk(text: &str) -> String {
    text.chars().filter(|c| is_cjk_char(*c)).collect()
}

fn dedupe_items(mut items: Vec<DetectedItem>) -> Vec<DetectedItem> {
    items.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<DetectedItem> = Vec::new();
    for it in items {
        let dup = kept.iter().any(|k| {
            let dx = (k.cx - it.cx).abs();
            let dy = (k.cy - it.cy).abs();
            dx < 40.0 && dy < 40.0 && keep_cjk(&k.text) == keep_cjk(&it.text)
        });
        if !dup {
            kept.push(it);
        }
    }
    kept
}

fn cluster_rows(items: &[DetectedItem]) -> Vec<Vec<DetectedItem>> {
    if items.is_empty() {
        return vec![];
    }
    let mut sorted: Vec<DetectedItem> = items.to_vec();
    sorted.sort_by(|a, b| {
        a.cy.partial_cmp(&b.cy)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut heights: Vec<f32> = sorted
        .iter()
        .map(|i| (i.y1 - i.y0).abs().max(8.0))
        .collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_h = heights[heights.len() / 2];
    let thresh = (median_h * 0.7).max(18.0);

    let mut rows: Vec<Vec<DetectedItem>> = Vec::new();
    for it in sorted {
        if let Some(last) = rows.last_mut() {
            let row_cy = last.iter().map(|x| x.cy).sum::<f32>() / last.len() as f32;
            if (it.cy - row_cy).abs() <= thresh {
                last.push(it);
                continue;
            }
        }
        rows.push(vec![it]);
    }
    rows
}

// Searches both above AND below the character for the nearest pinyin/tone text box.
// The previous logic only looked below, but real game layouts can put pinyin above the
// character - that mismatch was a direct cause of wrong sm/ym/tone colors.
// Once a candidate box is found, its recognized letters are fed through
// `crate::pinyin::parse_syllable` (reusing the existing initial-consonant table) to get
// the initial/final character counts, and the box width is split proportionally between
// them for region color voting. The tone digit is a superscript, so it is approximated as
// a small sub-region to the right and slightly above the main box.
fn sample_pronunciation_marks(
    prepared: &PreprocessedBoard,
    char_cx: f32,
    char_cy: f32,
    char_height: f32,
    pinyin_items: &[&DetectedItem],
) -> (ColorMark, ColorMark, ColorMark) {
    let mask_w = prepared.upscaled.width();
    let mask_h = prepared.upscaled.height();

    // Search radii scale with the character's own on-screen size instead of fixed pixel
    // constants: at high upscale factors the gap between a character and its pinyin line
    // can be several hundred pixels, so an absolute constant either matches nothing or
    // (at low scale) matches the wrong row.
    let h = char_height.max(8.0);
    let dx_tol = h * 1.1;
    let dy_min = h * 0.12;
    // Must stay well below the row-to-row spacing (roughly 2.3x char height in the
    // reference screenshot) or this ends up grabbing the next row's tone digit when the
    // current row's own tone box wasn't detected. Observed legitimate same-row gaps
    // topped out around 1.07x char height; the cross-row leak was at 1.45x.
    let dy_max = h * 1.3;

    // The tone digit is often detected as its own small box, separate from the main
    // sm/ym letters (it's visually separated as a superscript). Track the nearest
    // "letters" box and the nearest "digit-only" box independently, since picking a
    // single overall-nearest candidate can miss the tone box entirely when the letter
    // box happens to sit closer to the character.
    let mut best_letters: Option<&DetectedItem> = None;
    let mut best_letters_dy = f32::MAX;
    let mut best_digit: Option<&DetectedItem> = None;
    let mut best_digit_dy = f32::MAX;

    for it in pinyin_items {
        if (it.cx - char_cx).abs() >= dx_tol {
            continue;
        }
        let dy = (it.cy - char_cy).abs();
        if dy <= dy_min || dy >= dy_max {
            continue;
        }
        let cleaned: String = it
            .text
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        if cleaned.chars().all(|c| c.is_ascii_digit()) {
            if dy < best_digit_dy {
                best_digit_dy = dy;
                best_digit = Some(it);
            }
        } else if dy < best_letters_dy {
            best_letters_dy = dy;
            best_letters = Some(*it);
        }
    }

    // A standalone digit box's own whole-box vote is already a reliable tone color,
    // since the box is tight around just that one glyph.
    let standalone_tone_mark = best_digit.map(|it| it.mark);

    let Some(item) = best_letters else {
        // No letters box found: fall back to a region-vote band above, then below,
        // sized relative to character height rather than an absolute pixel guess.
        // Tone still prefers a standalone digit box if one was found.
        let half_w = h * 0.5;
        let band_h = h * 0.16;
        let offset = h * 0.85;
        let try_band = |center_y: f32| {
            preprocess::vote_mark(
                &prepared.hit_mask,
                &prepared.present_mask,
                mask_w,
                mask_h,
                char_cx - half_w,
                (center_y - band_h).max(0.0),
                char_cx + half_w,
                (center_y + band_h).max(0.0),
            )
        };
        let above = try_band(char_cy - offset);
        let mark = if above != ColorMark::Absent {
            above
        } else {
            try_band(char_cy + offset)
        };
        let tone = standalone_tone_mark.unwrap_or(mark);
        return (mark, mark, tone);
    };

    let cleaned: String = item
        .text
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();

    let parsed = crate::pinyin::parse_syllable(&cleaned);
    let sm_len = parsed.sm.chars().count();
    let ym_len = parsed.ym.chars().count();
    let total_letters = (sm_len + ym_len).max(1) as f32;

    // The detection box has padding/unclip margin around the actual glyphs, so splitting
    // proportionally from the raw box edge is systematically off. Trim to the real ink
    // span first, then split that.
    let (ink_x0, ink_x1) = trim_ink_x_range(&prepared.upscaled, item.x0, item.y0, item.x1, item.y1);
    let width = (ink_x1 - ink_x0).max(1.0);
    // Reserve a small slice on the right for the tone digit; split the rest by letter count.
    const TONE_FRAC: f32 = 0.22;
    let main_width = width * (1.0 - TONE_FRAC);
    let sm_x1 = ink_x0 + main_width * (sm_len as f32 / total_letters);

    let sm_mark = if sm_len == 0 {
        ColorMark::Absent
    } else {
        preprocess::vote_mark(
            &prepared.hit_mask,
            &prepared.present_mask,
            mask_w,
            mask_h,
            ink_x0,
            item.y0,
            sm_x1,
            item.y1,
        )
    };
    let ym_mark = if ym_len == 0 {
        ColorMark::Absent
    } else {
        preprocess::vote_mark(
            &prepared.hit_mask,
            &prepared.present_mask,
            mask_w,
            mask_h,
            sm_x1,
            item.y0,
            ink_x0 + main_width,
            item.y1,
        )
    };

    // Prefer a standalone tone-digit box when one was found; otherwise fall back to
    // carving out the upper-right corner of the letters box (superscript position).
    let tone_mark = standalone_tone_mark.unwrap_or_else(|| {
        let tone_x0 = ink_x0 + main_width;
        let tone_x1 = ink_x1.max(tone_x0 + 4.0);
        let tone_height = item.y1 - item.y0;
        let tone_y0 = (item.y0 - tone_height * 0.35).max(0.0);
        let tone_y1 = item.y0 + tone_height * 0.55;
        preprocess::vote_mark(
            &prepared.hit_mask,
            &prepared.present_mask,
            mask_w,
            mask_h,
            tone_x0,
            tone_y0,
            tone_x1,
            tone_y1,
        )
    });

    (sm_mark, ym_mark, tone_mark)
}

/// Trims a box's x-range down to where actual (non near-white) ink is found, since
/// detection boxes include padding/unclip margin that shifts naive proportional splits.
fn trim_ink_x_range(img: &image::RgbImage, x0: f32, y0: f32, x1: f32, y1: f32) -> (f32, f32) {
    const INK_BRIGHTNESS: u8 = 230;
    let w = img.width() as i32;
    let h = img.height() as i32;
    let xi0 = (x0.min(x1).floor() as i32).clamp(0, w - 1);
    let yi0 = (y0.min(y1).floor() as i32).clamp(0, h - 1);
    let xi1 = (x0.max(x1).ceil() as i32).clamp(xi0 + 1, w);
    let yi1 = (y0.max(y1).ceil() as i32).clamp(yi0 + 1, h);

    let mut left: Option<i32> = None;
    let mut right: Option<i32> = None;
    for x in xi0..xi1 {
        let mut has_ink = false;
        for y in yi0..yi1 {
            let px = img.get_pixel(x as u32, y as u32);
            if px[0].max(px[1]).max(px[2]) < INK_BRIGHTNESS {
                has_ink = true;
                break;
            }
        }
        if has_ink {
            if left.is_none() {
                left = Some(x);
            }
            right = Some(x);
        }
    }
    match (left, right) {
        (Some(l), Some(r)) => (l as f32, (r + 1) as f32),
        _ => (x0, x1),
    }
}

const WARN_NO_CHARS: &str = "未识别到疑似成语文字，请检查截图区域或调整放大倍数";
const WARN_TOO_MANY_A: &str = "该行识别到 ";
const WARN_TOO_MANY_B: &str = " 个字符（超过 4 个，已截断为前 4 个）：";
const WARN_TOO_FEW_A: &str = "该行仅识别到 ";
const WARN_TOO_FEW_B: &str = " 个字符（不足 4 个，已用占位符补齐）：";
const WARN_NO_GUESSES: &str = "未能识别出完整的成语猜测";
const WARN_NOT_IN_LEXICON_A: &str = "识别结果「";
const WARN_NOT_IN_LEXICON_B: &str = "」不在词库中，请核对";
const WARN_CORRECTED_A: &str = "已自动纠正「";
const WARN_CORRECTED_B: &str = "」→「";
const WARN_CORRECTED_C: &str = "」";
const WARN_AMBIGUOUS_A: &str = "识别结果「";
const WARN_AMBIGUOUS_B: &str = "」不在词库中，且存在多个相近候选，请人工核对";

// Corrects the assembled 4-character candidate word against the idiom lexicon: if it is
// not an exact match, look for lexicon entries of the same length that differ by exactly
// one character. A unique such match is auto-adopted; otherwise the original candidate is
// kept and a warning is surfaced instead of silently failing.
fn correct_with_lexicon(word: &str, idioms: &[Idiom]) -> (String, Option<String>) {
    if idioms.iter().any(|i| i.word == word) {
        return (word.to_string(), None);
    }

    let word_chars: Vec<char> = word.chars().collect();
    let target_len = word_chars.len();
    let mut candidates: Vec<&str> = Vec::new();
    for idiom in idioms {
        let cand_chars: Vec<char> = idiom.word.chars().collect();
        if cand_chars.len() != target_len {
            continue;
        }
        let diff = word_chars
            .iter()
            .zip(cand_chars.iter())
            .filter(|(a, b)| a != b)
            .count();
        if diff == 1 {
            candidates.push(idiom.word.as_str());
        }
    }
    candidates.sort_unstable();
    candidates.dedup();

    match candidates.len() {
        0 => (
            word.to_string(),
            Some(format!("{WARN_NOT_IN_LEXICON_A}{word}{WARN_NOT_IN_LEXICON_B}")),
        ),
        1 => {
            let corrected = candidates[0].to_string();
            let note = Some(format!(
                "{WARN_CORRECTED_A}{word}{WARN_CORRECTED_B}{corrected}{WARN_CORRECTED_C}"
            ));
            (corrected, note)
        }
        _ => (
            word.to_string(),
            Some(format!("{WARN_AMBIGUOUS_A}{word}{WARN_AMBIGUOUS_B}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    #[test]
    fn load_models_and_run_blank() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("ocr");
        if !dir.join(DET_NAME).exists() {
            eprintln!("skip: models missing at {}", dir.display());
            return;
        }
        set_model_dir(dir);
        ensure_engine().expect("load models");

        // tiny green PNG -> base64
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(64, 32, Rgb([40, 180, 80]));
        let mut png = Vec::new();
        {
            let mut cursor = std::io::Cursor::new(&mut png);
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut cursor, image::ImageFormat::Png)
                .unwrap();
        }
        let b64 = B64.encode(&png);
        let result = recognize_guess_board(&b64, 2.0).expect("recognize");
        assert!(result.raw_response.contains("local-paddle-ocr"));
        // blank board may yield zero guesses
        assert!(result.guesses.len() <= 8);
    }

    fn sample_idiom(word: &str) -> Idiom {
        Idiom {
            id: None,
            word: word.to_string(),
            pinyin: String::new(),
            explanation: String::new(),
            char_count: word.chars().count() as i32,
            chars: vec![],
            blind_eligible: false,
        }
    }

    #[test]
    fn correct_with_lexicon_fixes_single_char_typo() {
        let target = "\u{4e00}\u{5fc3}\u{4e00}\u{610f}"; // 一心一意
        let typo = "\u{4e00}\u{5fc3}\u{4e00}\u{4ebf}"; // 一心一亿 (last char wrong)
        let idioms = vec![sample_idiom(target)];
        let (word, note) = correct_with_lexicon(typo, &idioms);
        assert_eq!(word, target);
        assert!(note.is_some());
    }

    #[test]
    fn correct_with_lexicon_keeps_exact_match_silent() {
        let target = "\u{4e00}\u{5fc3}\u{4e00}\u{610f}"; // 一心一意
        let idioms = vec![sample_idiom(target)];
        let (word, note) = correct_with_lexicon(target, &idioms);
        assert_eq!(word, target);
        assert!(note.is_none());
    }

    // Regression test against a real screenshot (tests/fixtures/sample_guess_board.png).
    // Ground truth marks below were confirmed by directly sampling pixel colors in the
    // source image, not by eye - the on-screen colors for this game are quite subtle
    // (pale green/magenta ink on individual pinyin letters, no colored tile backgrounds).
    // Run: cargo test --release verify_with_real_screenshot -- --nocapture
    #[test]
    fn verify_with_real_screenshot() {
        let model_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("ocr");
        if !model_dir.join(DET_NAME).exists() {
            eprintln!("skip: models missing at {}", model_dir.display());
            return;
        }
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("sample_guess_board.png");
        if !fixture.exists() {
            eprintln!("skip: fixture missing at {}", fixture.display());
            return;
        }
        set_model_dir(model_dir);
        ensure_engine().expect("load models");

        let bytes = std::fs::read(&fixture).expect("read fixture");
        let b64 = B64.encode(&bytes);

        let started = std::time::Instant::now();
        let result = recognize_guess_board(&b64, 5.0).expect("recognize");
        let elapsed = started.elapsed();
        eprintln!("elapsed: {:?}", elapsed);
        assert!(
            elapsed.as_secs_f32() < 5.0,
            "local OCR should comfortably finish in a few seconds, took {elapsed:?}"
        );

        assert_eq!(result.guesses.len(), 2, "expected exactly 2 guess rows");

        let row1 = &result.guesses[0];
        assert_eq!(row1.word, "\u{5f3a}\u{6b7b}\u{8d56}\u{6d3b}"); // 强死赖活
        for (i, m) in row1.marks.iter().enumerate() {
            assert_eq!(m.char, "absent", "row1 pos{} char", i + 1);
            assert_eq!(m.sm, "absent", "row1 pos{} sm", i + 1);
            assert_eq!(m.ym, "absent", "row1 pos{} ym", i + 1);
            assert_eq!(m.tone, "absent", "row1 pos{} tone", i + 1);
        }

        let row2 = &result.guesses[1];
        assert_eq!(row2.word, "\u{653e}\u{8bde}\u{4e0d}\u{62d8}"); // 放诞不拘
        let expected = [
            ("present", "hit", "absent"),   // 放 fang4: f=present, ang=hit
            ("present", "absent", "absent"), // 诞 dan4: d=present
            ("absent", "absent", "hit"),    // 不 bu4: tone=hit
            ("absent", "absent", "hit"),    // 拘 ju1: tone=hit
        ];
        for (i, (m, (sm, ym, tone))) in row2.marks.iter().zip(expected).enumerate() {
            assert_eq!(m.char, "absent", "row2 pos{} char", i + 1);
            assert_eq!(m.sm, sm, "row2 pos{} sm", i + 1);
            assert_eq!(m.ym, ym, "row2 pos{} ym", i + 1);
            assert_eq!(m.tone, tone, "row2 pos{} tone", i + 1);
        }
    }
}
