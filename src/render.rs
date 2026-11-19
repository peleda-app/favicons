use std::io::Cursor;

use anyhow::{Context, ensure};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::codecs::webp::WebPEncoder;
use image::imageops::{self, FilterType as Resample};
use image::{ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, Limits, RgbaImage};
use resvg::{tiny_skia, usvg};

use crate::config::Render as RenderCfg;
use crate::contract::{MonoSource, OutputFormat, SIZES, Theme};

const MAX_ASPECT: u32 = 4;

#[derive(Debug, Clone)]
pub struct Variant {
    pub size: u32,
    pub data: Vec<u8>,
}

pub fn sniff_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head).to_ascii_lowercase();
    let t = text.trim_start_matches('\u{feff}').trim_start();
    (t.starts_with("<svg")
        || t.starts_with("<?xml")
        || t.starts_with("<!--")
        || t.starts_with("<!doctype svg"))
        && text.contains("<svg")
}

pub fn render_variants(bytes: &[u8], cfg: &RenderCfg) -> anyhow::Result<Vec<Variant>> {
    let (img, scalable) = decode_any(bytes, cfg)?;
    variants_from(img, scalable, cfg)
}

pub fn render_mono(
    bytes: &[u8],
    cfg: &RenderCfg,
    source: MonoSource,
) -> anyhow::Result<Vec<Variant>> {
    let (img, scalable) = decode_any(bytes, cfg)?;
    let mask = monochrome(&pad_square(img), source)?;
    variants_from(mask, scalable, cfg)
}

fn decode_any(bytes: &[u8], cfg: &RenderCfg) -> anyhow::Result<(RgbaImage, bool)> {
    ensure!(
        !is_compressed_container(bytes),
        "compressed containers are not accepted"
    );
    if sniff_svg(bytes) {
        ensure!(cfg.svg, "svg disabled");
        Ok((rasterize_svg(bytes, cfg)?, true))
    } else {
        Ok((decode_raster(bytes, cfg)?, false))
    }
}

fn color_distance(p: &image::Rgba<u8>, c: [f32; 3]) -> f32 {
    (0..3)
        .map(|i| (f32::from(p[i]) - c[i]).powi(2))
        .sum::<f32>()
        .sqrt()
        / 441.67
}

fn smoothstep(lo: f32, hi: f32, x: f32) -> f32 {
    let t = ((x - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn knockout_or_silhouette(img: &RgbaImage) -> Vec<u8> {
    let silhouette = || img.pixels().map(|p| p[3]).collect();
    let opaque: Vec<&image::Rgba<u8>> = img.pixels().filter(|p| p[3] >= 200).collect();
    if opaque.is_empty() {
        return silhouette();
    }
    let mut bins = std::collections::HashMap::<u16, (u32, [u32; 3])>::new();
    for p in &opaque {
        let key = (u16::from(p[0] >> 4) << 8) | (u16::from(p[1] >> 4) << 4) | u16::from(p[2] >> 4);
        let e = bins.entry(key).or_insert((0, [0; 3]));
        e.0 += 1;
        for c in 0..3 {
            e.1[c] += u32::from(p[c]);
        }
    }
    let Some((count, sum)) = bins.values().max_by_key(|(n, _)| *n) else {
        return silhouette();
    };
    let plate = sum.map(|v| v as f32 / *count as f32);
    let near = opaque
        .iter()
        .filter(|p| color_distance(p, plate) < 0.12)
        .count();
    let share = near as f32 / opaque.len() as f32;
    if !(0.5..=0.92).contains(&share) {
        return silhouette();
    }
    img.pixels()
        .map(|p| {
            let keep = 1.0 - smoothstep(0.10, 0.30, color_distance(p, plate));
            (keep * f32::from(p[3])).round() as u8
        })
        .collect()
}

pub fn monochrome(img: &RgbaImage, source: MonoSource) -> anyhow::Result<RgbaImage> {
    let (w, h) = img.dimensions();
    ensure!(w > 0 && h > 0, "empty image");
    let ink: Vec<u8> = match source {
        MonoSource::MaskIcon => img.pixels().map(|p| p[3]).collect(),
        MonoSource::Derived => {
            let border: Vec<image::Rgba<u8>> = (0..w)
                .flat_map(|x| [(x, 0), (x, h - 1)])
                .chain((0..h).flat_map(|y| [(0, y), (w - 1, y)]))
                .map(|(x, y)| *img.get_pixel(x, y))
                .collect();
            let transparent = border.iter().filter(|p| p[3] < 32).count();
            if transparent * 2 >= border.len() {
                knockout_or_silhouette(img)
            } else {
                let opaque: Vec<_> = border.iter().filter(|p| p[3] >= 200).collect();
                let pool: Vec<_> = if opaque.is_empty() {
                    border.iter().collect()
                } else {
                    opaque
                };
                let median = |c: usize| {
                    let mut v: Vec<u8> = pool.iter().map(|p| p[c]).collect();
                    v.sort_unstable();
                    f32::from(v[v.len() / 2])
                };
                let bg = [median(0), median(1), median(2)];
                img.pixels()
                    .map(|p| {
                        let d = (0..3)
                            .map(|c| (f32::from(p[c]) - bg[c]).powi(2))
                            .sum::<f32>()
                            .sqrt()
                            / 441.67;
                        let t = ((d - 0.10) / 0.20).clamp(0.0, 1.0);
                        let soft = t * t * (3.0 - 2.0 * t);
                        (soft * f32::from(p[3])).round() as u8
                    })
                    .collect()
            }
        }
    };
    let coverage = ink.iter().map(|a| u64::from(*a)).sum::<u64>() as f64
        / (255.0 * f64::from(w) * f64::from(h));
    ensure!(coverage >= 0.01, "no visible shape for a monochrome icon");
    ensure!(
        coverage <= 0.97,
        "not enough contrast for a monochrome icon"
    );
    let raw = ink.into_iter().flat_map(|a| [0, 0, 0, a]).collect();
    RgbaImage::from_raw(w, h, raw).context("building mask")
}

fn is_compressed_container(bytes: &[u8]) -> bool {
    const MAGICS: [&[u8]; 6] = [
        b"\x1f\x8b",
        b"BZh",
        b"\xfd7zXZ\x00",
        b"\x28\xb5\x2f\xfd",
        b"PK\x03\x04",
        b"\x04\x22\x4d\x18",
    ];
    MAGICS.iter().any(|m| bytes.starts_with(m))
        || (bytes.len() >= 2 && bytes[0] == 0x78 && matches!(bytes[1], 0x01 | 0x5e | 0x9c | 0xda))
}

fn decode_raster(bytes: &[u8], cfg: &RenderCfg) -> anyhow::Result<RgbaImage> {
    let format = image::guess_format(bytes).context("unknown image format")?;
    ensure!(
        matches!(
            format,
            ImageFormat::Png
                | ImageFormat::Ico
                | ImageFormat::Jpeg
                | ImageFormat::Gif
                | ImageFormat::WebP
                | ImageFormat::Bmp
        ),
        "image format {format:?} not allowed"
    );
    let mut limits = Limits::default();
    limits.max_image_width = Some(cfg.max_source_dimension);
    limits.max_image_height = Some(cfg.max_source_dimension);
    limits.max_alloc = Some(cfg.max_alloc_bytes);
    let (w, h) = ImageReader::with_format(Cursor::new(bytes), format)
        .into_dimensions()
        .context("reading image header")?;
    ensure!(
        w <= cfg.max_source_dimension && h <= cfg.max_source_dimension,
        "declared dimensions {w}x{h} exceed limit"
    );
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let img = reader.decode().context("decoding image")?.into_rgba8();
    let (w, h) = img.dimensions();
    ensure!(w > 0 && h > 0, "empty image");
    ensure!(
        w.max(h) <= w.min(h) * MAX_ASPECT,
        "aspect ratio too extreme"
    );
    Ok(img)
}

fn rasterize_svg(bytes: &[u8], cfg: &RenderCfg) -> anyhow::Result<RgbaImage> {
    ensure!(bytes.len() <= cfg.max_svg_bytes, "svg too large");
    ensure!(!contains_ci(bytes, b"<!entity"), "svg entities not allowed");
    ensure!(
        markup_depth(bytes) <= cfg.max_svg_depth,
        "svg nesting too deep"
    );

    let opt = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(bytes, &opt).context("parsing svg")?;
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    ensure!(
        w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0,
        "svg has no size"
    );
    ensure!(
        w.max(h) <= w.min(h) * MAX_ASPECT as f32,
        "aspect ratio too extreme"
    );

    let target = allowed_sizes(cfg).last().copied().unwrap_or(128);
    let scale = target as f32 / w.max(h);
    let (dx, dy) = (
        (target as f32 - w * scale) / 2.0,
        (target as f32 - h * scale) / 2.0,
    );
    let mut pixmap = tiny_skia::Pixmap::new(target, target).context("allocating canvas")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap_to_rgba(&pixmap))
}

fn allowed_sizes(cfg: &RenderCfg) -> Vec<u32> {
    let mut v: Vec<u32> = cfg
        .sizes
        .iter()
        .copied()
        .filter(|s| SIZES.contains(s))
        .collect();
    v.sort_unstable();
    v.dedup();
    if v.is_empty() { SIZES.to_vec() } else { v }
}

fn variants_from(img: RgbaImage, scalable: bool, cfg: &RenderCfg) -> anyhow::Result<Vec<Variant>> {
    let format = cfg.format;
    let allowed = allowed_sizes(cfg);
    let square = pad_square(img);
    let side = square.width();
    let mut sizes: Vec<u32> = allowed
        .iter()
        .copied()
        .filter(|s| scalable || side * 10 >= s * 9)
        .collect();
    if sizes.is_empty() {
        sizes.push(allowed[0]);
    }
    sizes
        .into_iter()
        .map(|size| {
            let resized = if size == side {
                square.clone()
            } else if side < SIZES[0] {
                imageops::resize(&square, size, size, Resample::Nearest)
            } else {
                imageops::resize(&square, size, size, Resample::Lanczos3)
            };
            Ok(Variant {
                size,
                data: encode(&resized, format)?,
            })
        })
        .collect()
}

fn pad_square(img: RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    if w == h {
        return img;
    }
    let side = w.max(h);
    let mut canvas = RgbaImage::new(side, side);
    imageops::overlay(
        &mut canvas,
        &img,
        i64::from((side - w) / 2),
        i64::from((side - h) / 2),
    );
    canvas
}

pub fn encode(img: &RgbaImage, format: OutputFormat) -> anyhow::Result<Vec<u8>> {
    match format {
        OutputFormat::Png => encode_png(img),
        OutputFormat::Webp => {
            let mut out = Vec::new();
            WebPEncoder::new_lossless(&mut out).write_image(
                img.as_raw(),
                img.width(),
                img.height(),
                ExtendedColorType::Rgba8,
            )?;
            Ok(out)
        }
    }
}

pub fn encode_png(img: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Best, FilterType::Adaptive)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )?;
    Ok(out)
}

fn pixmap_to_rgba(p: &tiny_skia::Pixmap) -> RgbaImage {
    let mut raw = Vec::with_capacity(p.data().len());
    for px in p.pixels() {
        let c = px.demultiply();
        raw.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    RgbaImage::from_raw(p.width(), p.height(), raw)
        .unwrap_or_else(|| RgbaImage::new(p.width(), p.height()))
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle))
}

pub fn markup_depth(bytes: &[u8]) -> usize {
    let (mut depth, mut max, mut i) = (0usize, 0usize, 0usize);
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &bytes[i..];
        if rest.starts_with(b"<!--") {
            i += find(rest, b"-->").map_or(rest.len(), |p| p + 3);
            continue;
        }
        if rest.starts_with(b"<![CDATA[") {
            i += find(rest, b"]]>").map_or(rest.len(), |p| p + 3);
            continue;
        }
        let end = find(rest, b">").map_or(rest.len(), |p| p + 1);
        let tag = &rest[..end];
        if tag.starts_with(b"</") {
            depth = depth.saturating_sub(1);
        } else if !tag.starts_with(b"<?") && !tag.starts_with(b"<!") && !tag.ends_with(b"/>") {
            depth += 1;
            max = max.max(depth);
        }
        i += end;
    }
    max
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn warning_icon(
    size: u32,
    theme: Theme,
    mono: bool,
    format: OutputFormat,
) -> anyhow::Result<Vec<u8>> {
    finish(warning_rgba(size, theme)?, mono, format)
}

fn finish(img: RgbaImage, mono: bool, format: OutputFormat) -> anyhow::Result<Vec<u8>> {
    if mono {
        encode(&monochrome(&img, MonoSource::Derived)?, format)
    } else {
        encode(&img, format)
    }
}

fn warning_rgba(size: u32, theme: Theme) -> anyhow::Result<RgbaImage> {
    let size = size.clamp(SIZES[0], *SIZES.last().unwrap_or(&128));
    let s = size as f32;
    let bg = match theme {
        Theme::Light => hsl(4.0, 0.76, 0.47),
        Theme::Dark => hsl(4.0, 0.80, 0.55),
    };
    let mut pixmap = tiny_skia::Pixmap::new(size, size).context("allocating canvas")?;
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    let id = tiny_skia::Transform::identity();
    paint.set_color_rgba8(bg[0], bg[1], bg[2], 255);
    let tile = rounded_rect(0.0, 0.0, s, s, s * 0.22).context("building path")?;
    pixmap.fill_path(&tile, &paint, tiny_skia::FillRule::Winding, id, None);

    paint.set_color_rgba8(255, 255, 255, 255);
    let w = (s * 0.16).max(2.0);
    let bar =
        rounded_rect((s - w) / 2.0, s * 0.16, w, s * 0.40, w / 2.0).context("building path")?;
    pixmap.fill_path(&bar, &paint, tiny_skia::FillRule::Winding, id, None);
    let dot = tiny_skia::PathBuilder::from_circle(s / 2.0, s * 0.76, w * 0.55)
        .context("building path")?;
    pixmap.fill_path(&dot, &paint, tiny_skia::FillRule::Winding, id, None);
    Ok(pixmap_to_rgba(&pixmap))
}

pub fn identicon(
    seed: &[u8; 32],
    size: u32,
    theme: Theme,
    mono: bool,
    format: OutputFormat,
) -> anyhow::Result<Vec<u8>> {
    finish(identicon_rgba(seed, size, theme)?, mono, format)
}

fn identicon_rgba(seed: &[u8; 32], size: u32, theme: Theme) -> anyhow::Result<RgbaImage> {
    let size = size.clamp(SIZES[0], *SIZES.last().unwrap_or(&128));
    let hue = f32::from(u16::from_le_bytes([seed[0], seed[1]]) % 360);
    let sat = 0.50 + f32::from(seed[2]) / 255.0 * 0.25;
    let (bg_l, fg) = match theme {
        Theme::Light => (0.46, hsl(hue, sat * 0.25, 0.97)),
        Theme::Dark => (0.30, hsl(hue, sat, 0.82)),
    };
    let bg = hsl(hue, sat, bg_l);

    let mut pixmap = tiny_skia::Pixmap::new(size, size).context("allocating canvas")?;
    let s = size as f32;
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(bg[0], bg[1], bg[2], 255);
    let path = rounded_rect(0.0, 0.0, s, s, s * 0.22).context("building path")?;
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    let pad = s * 0.2;
    let cell = (s - 2.0 * pad) / 5.0;
    paint.set_color_rgba8(fg[0], fg[1], fg[2], 255);
    paint.anti_alias = false;
    let bits = u32::from_le_bytes([seed[3], seed[4], seed[5], seed[6]]);
    let mut lit = 0;
    for row in 0..5u32 {
        for col in 0..3u32 {
            if bits >> (row * 3 + col) & 1 == 0 {
                continue;
            }
            lit += 1;
            for c in [col, 4 - col] {
                let x = (pad + c as f32 * cell).round();
                let y = (pad + row as f32 * cell).round();
                let w = (pad + (c + 1) as f32 * cell).round() - x;
                let h = (pad + (row + 1) as f32 * cell).round() - y;
                if let Some(r) = tiny_skia::Rect::from_xywh(x, y, w.max(1.0), h.max(1.0)) {
                    pixmap.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
                }
            }
        }
    }
    if lit == 0 {
        let c = (pad + 2.0 * cell).round();
        if let Some(r) =
            tiny_skia::Rect::from_xywh(c, c, cell.round().max(1.0), cell.round().max(1.0))
        {
            pixmap.fill_rect(r, &paint, tiny_skia::Transform::identity(), None);
        }
    }
    Ok(pixmap_to_rgba(&pixmap))
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

fn hsl(h: f32, s: f32, l: f32) -> [u8; 3] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
pub fn ensure_dimensions(data: &[u8], expected: u32) -> anyhow::Result<()> {
    let img = image::load_from_memory(data)?;
    if img.width() != expected || img.height() != expected {
        anyhow::bail!("unexpected dimensions {}x{}", img.width(), img.height());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RenderCfg {
        RenderCfg::default()
    }

    fn png_of(w: u32, h: u32) -> Vec<u8> {
        encode_png(&RgbaImage::from_pixel(
            w,
            h,
            image::Rgba([200, 10, 10, 255]),
        ))
        .unwrap()
    }

    #[test]
    fn identicon_is_deterministic_and_sized() {
        let seed = *blake3::hash(b"example.com").as_bytes();
        for size in SIZES {
            let a = identicon(&seed, size, Theme::Light, false, OutputFormat::Webp).unwrap();
            assert_eq!(
                a,
                identicon(&seed, size, Theme::Light, false, OutputFormat::Webp).unwrap()
            );
            assert_ne!(
                a,
                identicon(&seed, size, Theme::Dark, false, OutputFormat::Webp).unwrap()
            );
            ensure_dimensions(&a, size).unwrap();
        }
    }

    #[test]
    fn warning_icon_is_red_with_white_mark() {
        for size in SIZES {
            for theme in [Theme::Light, Theme::Dark] {
                let data = warning_icon(size, theme, false, OutputFormat::Png).unwrap();
                assert_eq!(
                    data,
                    warning_icon(size, theme, false, OutputFormat::Png).unwrap()
                );
                let img = image::load_from_memory(&data).unwrap().into_rgba8();
                assert_eq!(img.dimensions(), (size, size));
                let edge = img.get_pixel(size / 2, size / 12).0;
                assert!(
                    edge[0] > 180 && edge[1] < 90 && edge[2] < 90,
                    "{size} {theme:?} bg {edge:?}"
                );
                let mark = img.get_pixel(size / 2, size * 2 / 5).0;
                assert!(
                    mark.iter().all(|c| *c > 220),
                    "{size} {theme:?} mark {mark:?}"
                );
                let gap = img.get_pixel(size / 2, size * 62 / 100).0;
                assert!(gap[0] > 180 && gap[1] < 120, "{size} {theme:?} gap {gap:?}");
            }
        }
    }

    fn alpha_at(img: &RgbaImage, x: u32, y: u32) -> u8 {
        img.get_pixel(x, y)[3]
    }

    #[test]
    fn derives_mark_from_tile_and_silhouette_from_transparent_logo() {
        let mut tile = RgbaImage::from_pixel(64, 64, image::Rgba([220, 30, 30, 255]));
        for (x, y, p) in tile.enumerate_pixels_mut() {
            if (x as i32 - 32).pow(2) + (y as i32 - 32).pow(2) < 144 {
                *p = image::Rgba([255, 255, 255, 255]);
            }
        }
        let m = monochrome(&tile, MonoSource::Derived).unwrap();
        assert_eq!(alpha_at(&m, 32, 32), 255);
        assert_eq!(alpha_at(&m, 2, 2), 0);
        assert!(m.pixels().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));

        let mut logo = RgbaImage::new(64, 64);
        for (x, y, p) in logo.enumerate_pixels_mut() {
            if (x as i32 - 32).pow(2) + (y as i32 - 32).pow(2) < 400 {
                *p = image::Rgba([30, 120, 220, 255]);
            }
        }
        let m = monochrome(&logo, MonoSource::Derived).unwrap();
        assert_eq!(alpha_at(&m, 32, 32), 255);
        assert_eq!(alpha_at(&m, 1, 1), 0);
    }

    #[test]
    fn knocks_details_out_of_a_plate() {
        let mut img = RgbaImage::new(64, 64);
        for (x, y, p) in img.enumerate_pixels_mut() {
            let d = (x as i32 - 32).pow(2) + (y as i32 - 32).pow(2);
            if d < 100 {
                *p = image::Rgba([220, 40, 40, 255]);
            } else if d < 900 {
                *p = image::Rgba([255, 255, 255, 255]);
            }
        }
        let m = monochrome(&img, MonoSource::Derived).unwrap();
        assert_eq!(alpha_at(&m, 32, 50), 255, "plate stays");
        assert_eq!(alpha_at(&m, 32, 32), 0, "detail is cut out");
        assert_eq!(alpha_at(&m, 1, 1), 0, "outside stays empty");
    }

    #[test]
    fn rejects_shapeless_monochrome() {
        let flat = RgbaImage::from_pixel(32, 32, image::Rgba([10, 200, 10, 255]));
        assert!(monochrome(&flat, MonoSource::Derived).is_err());
        assert!(monochrome(&RgbaImage::new(32, 32), MonoSource::MaskIcon).is_err());
    }

    #[test]
    fn renders_mask_icon_svg_as_black_alpha() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 2h12v12H2z" fill="#000"/></svg>"##;
        let v = render_mono(svg, &cfg(), MonoSource::MaskIcon).unwrap();
        assert_eq!(v.iter().map(|v| v.size).collect::<Vec<_>>(), SIZES);
        let img = image::load_from_memory(&v[3].data).unwrap().into_rgba8();
        assert_eq!(img.get_pixel(64, 64).0, [0, 0, 0, 255]);
        assert_eq!(img.get_pixel(4, 4)[3], 0);
    }

    #[test]
    fn generated_icons_have_mono_versions() {
        let seed = *blake3::hash(b"example.com").as_bytes();
        for data in [
            identicon(&seed, 64, Theme::Light, true, OutputFormat::Png).unwrap(),
            warning_icon(64, Theme::Light, true, OutputFormat::Png).unwrap(),
        ] {
            let img = image::load_from_memory(&data).unwrap().into_rgba8();
            assert!(img.pixels().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
            assert!(img.pixels().any(|p| p[3] == 255));
            assert!(img.pixels().any(|p| p[3] == 0));
        }
    }

    #[test]
    fn produces_only_sizes_not_above_source() {
        let v = render_variants(&png_of(48, 48), &cfg()).unwrap();
        assert_eq!(v.iter().map(|v| v.size).collect::<Vec<_>>(), [16, 32]);
        let near = render_variants(&png_of(120, 120), &cfg()).unwrap();
        assert_eq!(near.iter().map(|v| v.size).collect::<Vec<_>>(), SIZES);
        for variant in &v {
            ensure_dimensions(&variant.data, variant.size).unwrap();
        }
        let tiny = render_variants(&png_of(8, 8), &cfg()).unwrap();
        assert_eq!(tiny.iter().map(|v| v.size).collect::<Vec<_>>(), [16]);
    }

    #[test]
    fn honours_configured_sizes() {
        let only_128 = RenderCfg {
            sizes: vec![128],
            ..cfg()
        };
        let v = render_variants(&png_of(48, 48), &only_128).unwrap();
        assert_eq!(v.iter().map(|v| v.size).collect::<Vec<_>>(), [128]);
        ensure_dimensions(&v[0].data, 128).unwrap();
        let some = RenderCfg {
            sizes: vec![32, 128],
            ..cfg()
        };
        let v = render_variants(&png_of(256, 256), &some).unwrap();
        assert_eq!(v.iter().map(|v| v.size).collect::<Vec<_>>(), [32, 128]);
        let v = render_variants(&png_of(64, 64), &some).unwrap();
        assert_eq!(v.iter().map(|v| v.size).collect::<Vec<_>>(), [32]);
    }

    #[test]
    fn pads_non_square_images() {
        let v = render_variants(&png_of(128, 64), &cfg()).unwrap();
        assert_eq!(v.last().unwrap().size, 128);
        ensure_dimensions(&v.last().unwrap().data, 128).unwrap();
    }

    #[test]
    fn rejects_oversized_and_hostile_inputs() {
        assert!(render_variants(&png_of(2048, 2048), &cfg()).is_err());
        assert!(render_variants(&png_of(400, 20), &cfg()).is_err());
        assert!(render_variants(b"<html>not an image</html>", &cfg()).is_err());
        assert!(render_variants(b"GIF89a\x00\x00", &cfg()).is_err());
        let tiff = b"II*\x00\x08\x00\x00\x00";
        assert!(render_variants(tiff, &cfg()).is_err());
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &b in data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xedb8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn png_claiming(w: u32, h: u32) -> Vec<u8> {
        let mut png = png_of(64, 64);
        png[16..20].copy_from_slice(&w.to_be_bytes());
        png[20..24].copy_from_slice(&h.to_be_bytes());
        let crc = crc32(&png[12..29]);
        png[29..33].copy_from_slice(&crc.to_be_bytes());
        png
    }

    fn assert_rejected_fast(name: &str, data: &[u8]) {
        let started = std::time::Instant::now();
        assert!(
            render_variants(data, &cfg()).is_err(),
            "{name} must be rejected"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(1),
            "{name} must be rejected before decoding"
        );
    }

    #[test]
    fn rejects_decompression_bombs() {
        let png = png_claiming(60_000, 60_000);
        assert!(png.len() < 2_048);
        assert_rejected_fast("png 60000x60000", &png);
        assert_rejected_fast("png 1025x1", &png_claiming(1025, 1));

        let mut ico = vec![0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0];
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&22u32.to_le_bytes());
        ico.extend_from_slice(&png);
        assert_rejected_fast("ico wrapping png bomb", &ico);

        let dims: u32 = 0x3fff | (0x3fff << 14) | (1 << 28);
        let mut vp8l = vec![0x2f];
        vp8l.extend_from_slice(&dims.to_le_bytes());
        vp8l.extend_from_slice(&[0u8; 64]);
        let mut webp = b"RIFF".to_vec();
        webp.extend_from_slice(&((4 + 8 + vp8l.len()) as u32).to_le_bytes());
        webp.extend_from_slice(b"WEBPVP8L");
        webp.extend_from_slice(&(vp8l.len() as u32).to_le_bytes());
        webp.extend_from_slice(&vp8l);
        assert_rejected_fast("webp 16384x16384", &webp);

        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&[0xff, 0xff, 0xff, 0xff, 0, 0, 0]);
        gif.extend_from_slice(&[
            0x2c, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff, 0, 2, 2, 0x4c, 0x01, 0, 0x3b,
        ]);
        assert_rejected_fast("gif 65535x65535", &gif);
    }

    #[test]
    fn rejects_compressed_wrappers() {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#;
        let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
        gz.extend_from_slice(svg);
        assert_rejected_fast("svgz", &gz);
        assert_rejected_fast("zlib", &[0x78, 0x9c, 1, 2, 3]);
        assert_rejected_fast("zip", b"PK\x03\x04rest");
    }

    #[test]
    fn encodes_webp_by_default() {
        let v = render_variants(&png_of(32, 32), &cfg()).unwrap();
        let data = &v[0].data;
        assert_eq!(&data[..4], b"RIFF");
        assert_eq!(&data[8..12], b"WEBP");
        let png_cfg = RenderCfg {
            format: OutputFormat::Png,
            ..cfg()
        };
        let p = render_variants(&png_of(32, 32), &png_cfg).unwrap();
        assert_eq!(&p[0].data[..4], b"\x89PNG");
    }

    #[test]
    fn renders_svg_at_all_sizes() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="#f00"/></svg>"##;
        let v = render_variants(svg, &cfg()).unwrap();
        assert_eq!(v.iter().map(|v| v.size).collect::<Vec<_>>(), SIZES);
    }

    #[test]
    fn svg_cannot_reach_filesystem_or_network() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10">
            <image href="/etc/passwd" width="10" height="10"/>
            <image xlink:href="https://169.254.169.254/latest/meta-data" width="10" height="10"/>
            <rect width="10" height="10" fill="#0f0"/></svg>"##;
        assert!(render_variants(svg, &cfg()).is_ok());
    }

    #[test]
    fn rejects_entity_expansion_and_deep_nesting() {
        let lol = br#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "aaaaaaaa"><!ENTITY b "&a;&a;&a;&a;">]><svg xmlns="http://www.w3.org/2000/svg">&b;</svg>"#;
        assert!(render_variants(lol, &cfg()).is_err());
        let deep = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">{}{}</svg>"#,
            "<g>".repeat(5000),
            "</g>".repeat(5000)
        );
        assert!(render_variants(deep.as_bytes(), &cfg()).is_err());
    }

    #[test]
    fn measures_markup_depth() {
        assert_eq!(markup_depth(b"<a><b/><c><d></d></c></a>"), 3);
        assert_eq!(markup_depth(b"<?xml?><!-- <x><y> --><a></a>"), 1);
    }
}
