//! 截图预处理：放大 + HSV 颜色掩码 + 整图二值化。
//!
//! 颜色判定不再渲染成单独的“通道图”反复跑 OCR，而是保留布尔掩码，
//! 供上层对任意矩形区域做“区域颜色投票”（而不是单点采样）。

use image::{imageops::FilterType, DynamicImage, Rgb, RgbImage};

/// 钳制后的放大倍数（默认 5.0 = 500%）
pub fn clamp_scale(scale: f32) -> f32 {
    if !scale.is_finite() {
        return 5.0;
    }
    scale.clamp(1.0, 8.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMark {
    Hit,
    Present,
    Absent,
}

impl ColorMark {
    pub fn as_str(self) -> &'static str {
        match self {
            ColorMark::Hit => "hit",
            ColorMark::Present => "present",
            ColorMark::Absent => "absent",
        }
    }
}

pub struct PreprocessedBoard {
    pub scale: f32,
    /// 放大后的彩色图（区域颜色投票的坐标系基准）
    pub upscaled: RgbImage,
    /// 整图二值化结果，唯一一次送入 OCR 检测的图
    pub ocr_bin: RgbImage,
    /// 命中（绿）颜色掩码，行主序，长度 = upscaled.width() * upscaled.height()
    pub hit_mask: Vec<bool>,
    /// 存在但位置错误（紫/粉）颜色掩码
    pub present_mask: Vec<bool>,
}

/// 从原始字节解码并预处理。
pub fn preprocess_image(bytes: &[u8], scale: f32) -> Result<PreprocessedBoard, String> {
    let scale = clamp_scale(scale);
    let img = image::load_from_memory(bytes).map_err(|e| format!("解码图片失败: {e}"))?;
    let upscaled = upscale_image(&img, scale);
    let (hit_mask, present_mask) = build_color_masks(&upscaled);
    let ocr_bin = grayscale_binarize(&upscaled);

    Ok(PreprocessedBoard {
        scale,
        upscaled,
        ocr_bin,
        hit_mask,
        present_mask,
    })
}

fn upscale_image(img: &DynamicImage, scale: f32) -> RgbImage {
    let rgb = img.to_rgb8();
    if (scale - 1.0).abs() < 0.01 {
        return rgb;
    }
    let w = ((rgb.width() as f32) * scale).round().max(1.0) as u32;
    let h = ((rgb.height() as f32) * scale).round().max(1.0) as u32;
    image::imageops::resize(&rgb, w, h, FilterType::Lanczos3)
}

/// RGB → HSV（H: 0..=360, S/V: 0..=1）
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let delta = max - min;

    let h = if delta < 1e-6 {
        0.0
    } else if (max - rf).abs() < 1e-6 {
        60.0 * (((gf - bf) / delta) % 6.0)
    } else if (max - gf).abs() < 1e-6 {
        60.0 * (((bf - rf) / delta) + 2.0)
    } else {
        60.0 * (((rf - gf) / delta) + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max < 1e-6 { 0.0 } else { delta / max };
    (h, s, max)
}

fn is_hit(h: f32, s: f32, v: f32) -> bool {
    s >= 0.18 && v >= 0.25 && h >= 70.0 && h <= 170.0
}

fn is_present(h: f32, s: f32, v: f32) -> bool {
    if s < 0.18 || v < 0.25 {
        return false;
    }
    // 紫 / 粉（含偏红粉），280~360 与 0~15 已覆盖偏红区间，无需重复区间
    (h >= 280.0 && h <= 360.0) || (h >= 0.0 && h <= 15.0)
}

/// 在放大图上采样单个像素点，判定 hit/present/absent。
/// 主流程已改为区域投票（见 [`vote_mark`]），这个函数保留给极小区域兜底与测试使用。
#[allow(dead_code)]
pub fn classify_pixel(img: &RgbImage, x: u32, y: u32) -> ColorMark {
    let w = img.width().saturating_sub(1);
    let h = img.height().saturating_sub(1);
    let cx = x.min(w);
    let cy = y.min(h);
    let px = *img.get_pixel(cx, cy);
    let (hh, s, v) = rgb_to_hsv(px[0], px[1], px[2]);
    if is_hit(hh, s, v) {
        ColorMark::Hit
    } else if is_present(hh, s, v) {
        ColorMark::Present
    } else {
        ColorMark::Absent
    }
}

/// 在一个矩形区域内统计命中/存在掩码占比，取占比较高且过阈值的一方；
/// 都不明显则记为 Absent。比单点采样更抗抗锯齿/字体渲染噪声。
pub fn vote_mark(
    hit_mask: &[bool],
    present_mask: &[bool],
    width: u32,
    height: u32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
) -> ColorMark {
    if width == 0 || height == 0 {
        return ColorMark::Absent;
    }
    let w = width as i32;
    let h = height as i32;
    let xi0 = (x0.min(x1).floor() as i32).clamp(0, w - 1);
    let yi0 = (y0.min(y1).floor() as i32).clamp(0, h - 1);
    let xi1 = (x0.max(x1).ceil() as i32).clamp(xi0 + 1, w);
    let yi1 = (y0.max(y1).ceil() as i32).clamp(yi0 + 1, h);

    let mut hit_count = 0u32;
    let mut present_count = 0u32;
    let mut total = 0u32;
    for y in yi0..yi1 {
        let row_base = (y * w) as usize;
        for x in xi0..xi1 {
            let idx = row_base + x as usize;
            total += 1;
            if hit_mask[idx] {
                hit_count += 1;
            } else if present_mask[idx] {
                present_count += 1;
            }
        }
    }
    if total == 0 {
        return ColorMark::Absent;
    }

    const MIN_RATIO: f32 = 0.06;
    let hit_ratio = hit_count as f32 / total as f32;
    let present_ratio = present_count as f32 / total as f32;
    if hit_ratio >= present_ratio && hit_ratio >= MIN_RATIO {
        ColorMark::Hit
    } else if present_ratio > hit_ratio && present_ratio >= MIN_RATIO {
        ColorMark::Present
    } else {
        ColorMark::Absent
    }
}

fn build_color_masks(img: &RgbImage) -> (Vec<bool>, Vec<bool>) {
    let n = (img.width() * img.height()) as usize;
    let mut hit = vec![false; n];
    let mut present = vec![false; n];
    for (i, px) in img.pixels().enumerate() {
        let (h, s, v) = rgb_to_hsv(px[0], px[1], px[2]);
        if is_hit(h, s, v) {
            hit[i] = true;
        } else if is_present(h, s, v) {
            present[i] = true;
        }
    }
    // 轻度膨胀，避免细笔画抗锯齿边缘丢色
    (
        dilate_mask(&hit, img.width(), img.height(), 1),
        dilate_mask(&present, img.width(), img.height(), 1),
    )
}

fn dilate_mask(mask: &[bool], width: u32, height: u32, radius: i32) -> Vec<bool> {
    if radius <= 0 {
        return mask.to_vec();
    }
    let w = width as i32;
    let h = height as i32;
    let mut out = vec![false; mask.len()];
    for y in 0..h {
        for x in 0..w {
            let mut on = false;
            'outer: for dy in -radius..=radius {
                for dx in -radius..=radius {
                    let nx = x + dx;
                    let ny = y + dy;
                    if nx < 0 || ny < 0 || nx >= w || ny >= h {
                        continue;
                    }
                    if mask[(ny * w + nx) as usize] {
                        on = true;
                        break 'outer;
                    }
                }
            }
            out[(y * w + x) as usize] = on;
        }
    }
    out
}

fn grayscale_binarize(img: &RgbImage) -> RgbImage {
    let w = img.width();
    let h = img.height();
    let mut out = RgbImage::new(w, h);
    // Otsu 阈值
    let mut hist = [0u32; 256];
    let mut grays = Vec::with_capacity((w * h) as usize);
    for px in img.pixels() {
        let g = ((px[0] as u16 + px[1] as u16 + px[2] as u16) / 3) as u8;
        hist[g as usize] += 1;
        grays.push(g);
    }
    let threshold = otsu_threshold(&hist, grays.len() as u32);
    for (i, g) in grays.into_iter().enumerate() {
        let v = if g <= threshold { 0 } else { 255 };
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        out.put_pixel(x, y, Rgb([v, v, v]));
    }
    out
}

fn otsu_threshold(hist: &[u32; 256], total: u32) -> u8 {
    if total == 0 {
        return 128;
    }
    let mut sum_all = 0f64;
    for (i, &c) in hist.iter().enumerate() {
        sum_all += i as f64 * c as f64;
    }
    let mut sum_b = 0f64;
    let mut w_b = 0u32;
    let mut max_var = -1.0f64;
    let mut threshold = 128u8;
    for t in 0..256 {
        w_b += hist[t];
        if w_b == 0 {
            continue;
        }
        let w_f = total - w_b;
        if w_f == 0 {
            break;
        }
        sum_b += t as f64 * hist[t] as f64;
        let m_b = sum_b / w_b as f64;
        let m_f = (sum_all - sum_b) / w_f as f64;
        let var = w_b as f64 * w_f as f64 * (m_b - m_f) * (m_b - m_f);
        if var > max_var {
            max_var = var;
            threshold = t as u8;
        }
    }
    threshold
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_scale_bounds() {
        assert!((clamp_scale(5.0) - 5.0).abs() < 1e-6);
        assert!((clamp_scale(0.5) - 1.0).abs() < 1e-6);
        assert!((clamp_scale(99.0) - 8.0).abs() < 1e-6);
    }

    #[test]
    fn classify_green_hit() {
        let mut img = RgbImage::new(1, 1);
        img.put_pixel(0, 0, Rgb([40, 180, 80]));
        assert_eq!(classify_pixel(&img, 0, 0), ColorMark::Hit);
    }

    #[test]
    fn vote_mark_region() {
        let width = 4u32;
        let height = 4u32;
        let mut hit_mask = vec![false; (width * height) as usize];
        for y in 0..2u32 {
            for x in 0..2u32 {
                hit_mask[(y * width + x) as usize] = true;
            }
        }
        let present_mask = vec![false; (width * height) as usize];

        let hit = vote_mark(&hit_mask, &present_mask, width, height, 0.0, 0.0, 2.0, 2.0);
        assert_eq!(hit, ColorMark::Hit);

        let absent = vote_mark(&hit_mask, &present_mask, width, height, 2.0, 2.0, 4.0, 4.0);
        assert_eq!(absent, ColorMark::Absent);
    }

    #[test]
    fn preprocess_produces_masks_matching_image_size() {
        let img: image::ImageBuffer<Rgb<u8>, Vec<u8>> = image::ImageBuffer::from_fn(20, 10, |x, _| {
            if x < 7 {
                Rgb([40, 180, 80])
            } else if x < 14 {
                Rgb([200, 80, 200])
            } else {
                Rgb([140, 140, 140])
            }
        });
        let mut png = Vec::new();
        {
            let mut cursor = std::io::Cursor::new(&mut png);
            image::DynamicImage::ImageRgb8(img)
                .write_to(&mut cursor, image::ImageFormat::Png)
                .unwrap();
        }
        let prepared = preprocess_image(&png, 3.0).unwrap();
        assert_eq!(prepared.upscaled.width(), 60);
        assert_eq!(prepared.ocr_bin.width(), 60);
        assert_eq!(prepared.hit_mask.len(), 60 * 30);
        assert_eq!(prepared.present_mask.len(), 60 * 30);
    }
}
