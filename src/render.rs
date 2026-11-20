use std::io::Cursor;

use anyhow::{Context, ensure};
use image::imageops::{self, FilterType as Resample};
use image::{ImageFormat, ImageReader, Limits, RgbaImage};
use resvg::usvg;

use crate::config::Render as RenderCfg;
use crate::contract::MonoSource;
use crate::trace;
use crate::vector::{self, VIEW, Vector, VectorOrigin};

const MAX_ASPECT: u32 = 4;
const WORK_PX: u32 = 512;
const MAX_PALETTE: usize = 12;

pub fn vectorize(bytes: &[u8], cfg: &RenderCfg) -> anyhow::Result<Vector> {
    ensure!(
        !is_compressed_container(bytes),
        "compressed containers are not accepted"
    );
    if sniff_svg(bytes) {
        let tree = checked_svg(bytes, cfg)?;
        let original = vector::render(&tree, VIEW)?;
        if let Ok(clean) = vector::clean_tree(&tree, false, false)
            && clean.svg.len() <= cfg.max_output_svg_bytes
            && fidelity(&original, &clean.svg)? <= allowed_loss(cfg, clean.lossy)
        {
            return Ok(Vector {
                svg: finish(&clean.svg, cfg)?,
                origin: VectorOrigin::Original,
            });
        }
        let raster = vector::render(&tree, WORK_PX)?;
        return Ok(Vector {
            svg: finish(&trace_color(&raster, cfg)?, cfg)?,
            origin: VectorOrigin::Traced,
        });
    }
    let img = prepare_raster(decode_raster(bytes, cfg)?, cfg);
    Ok(Vector {
        svg: finish(&trace_color(&img, cfg)?, cfg)?,
        origin: VectorOrigin::Traced,
    })
}

pub fn vectorize_mono(bytes: &[u8], cfg: &RenderCfg, source: MonoSource) -> anyhow::Result<Vector> {
    ensure!(
        !is_compressed_container(bytes),
        "compressed containers are not accepted"
    );
    let rgba = if sniff_svg(bytes) {
        let tree = checked_svg(bytes, cfg)?;
        if source == MonoSource::MaskIcon
            && let Ok(clean) = vector::clean_tree(&tree, true, false)
            && clean.svg.len() <= cfg.max_output_svg_bytes
        {
            let original = monochrome(&vector::render(&tree, VIEW)?, MonoSource::MaskIcon)?;
            if fidelity(&original, &clean.svg)? <= allowed_loss(cfg, clean.lossy) {
                return Ok(Vector {
                    svg: finish(&clean.svg, cfg)?,
                    origin: VectorOrigin::Original,
                });
            }
        }
        vector::render(&tree, WORK_PX)?
    } else {
        prepare_raster(decode_raster(bytes, cfg)?, cfg)
    };
    let mask = monochrome(&rgba, source)?;
    Ok(Vector {
        svg: finish(&trace_mask(&mask, cfg)?, cfg)?,
        origin: VectorOrigin::Traced,
    })
}

fn finish(svg: &str, cfg: &RenderCfg) -> anyhow::Result<String> {
    let fitted = vector::fit(svg)?;
    ensure!(
        fitted.len() <= cfg.max_output_svg_bytes,
        "svg too large after fitting"
    );
    Ok(fitted)
}

fn allowed_loss(cfg: &RenderCfg, lossy: bool) -> f32 {
    if lossy {
        cfg.svg_fidelity / 2.0
    } else {
        cfg.svg_fidelity
    }
}

fn trace_side(cfg: &RenderCfg) -> u32 {
    cfg.trace_max_px.clamp(64, 512)
}

fn fidelity(reference: &RgbaImage, svg: &str) -> anyhow::Result<f32> {
    let rendered = vector::render(&vector::parse(svg.as_bytes())?, VIEW)?;
    Ok(vector::difference(reference, &rendered))
}

fn checked_svg(bytes: &[u8], cfg: &RenderCfg) -> anyhow::Result<usvg::Tree> {
    ensure!(cfg.svg, "svg disabled");
    ensure!(bytes.len() <= cfg.max_svg_bytes, "svg too large");
    ensure!(!contains_ci(bytes, b"<!entity"), "svg entities not allowed");
    ensure!(
        markup_depth(bytes) <= cfg.max_svg_depth,
        "svg nesting too deep"
    );
    let tree = vector::parse(bytes)?;
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
    Ok(tree)
}

fn prepare_raster(img: RgbaImage, cfg: &RenderCfg) -> RgbaImage {
    let square = pad_square(img);
    let max = trace_side(cfg);
    if square.width() > max {
        imageops::resize(&square, max, max, Resample::Lanczos3)
    } else {
        square
    }
}

fn reference(img: &RgbaImage) -> RgbaImage {
    match img.width() {
        w if w == VIEW => img.clone(),
        w if w < VIEW => imageops::resize(img, VIEW, VIEW, Resample::Nearest),
        _ => imageops::resize(img, VIEW, VIEW, Resample::Lanczos3),
    }
}

fn resize_premultiplied(img: &RgbaImage, side: u32) -> RgbaImage {
    if img.width() == side && img.height() == side {
        return img.clone();
    }
    let mut pre = img.clone();
    for p in pre.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            p[c] = ((u16::from(p[c]) * a + 127) / 255) as u8;
        }
    }
    let mut out = imageops::resize(&pre, side, side, Resample::CatmullRom);
    for p in out.pixels_mut() {
        let a = u16::from(p[3]);
        for c in 0..3 {
            if let Some(v) = (u16::from(p[c]) * 255 + a / 2).checked_div(a) {
                p[c] = v.min(255) as u8;
            }
        }
    }
    out
}

fn rgb_distance(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f32>().sqrt() / 441.67
}

fn is_between(c: &[f32; 3], a: &[f32; 3], b: &[f32; 3]) -> bool {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let len2: f32 = ab.iter().map(|v| v * v).sum();
    if len2 < 1.0 {
        return false;
    }
    let t = (0..3).map(|i| (c[i] - a[i]) * ab[i]).sum::<f32>() / len2;
    if !(0.1..=0.9).contains(&t) {
        return false;
    }
    let on_segment = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    rgb_distance(c, &on_segment) < 0.06
}

pub fn quantize(img: &RgbaImage, max_colors: usize) -> RgbaImage {
    let mut hist = std::collections::HashMap::<u16, (u32, [u32; 3])>::new();
    let mut opaque = 0u32;
    for p in img.pixels().filter(|p| p[3] >= 128) {
        opaque += 1;
        let key = (u16::from(p[0] >> 3) << 10) | (u16::from(p[1] >> 3) << 5) | u16::from(p[2] >> 3);
        let e = hist.entry(key).or_insert((0, [0; 3]));
        e.0 += 1;
        for c in 0..3 {
            e.1[c] += u32::from(p[c]);
        }
    }
    let mut bins: Vec<(u32, [f32; 3])> = hist
        .into_values()
        .map(|(n, sum)| (n, sum.map(|v| v as f32 / n as f32)))
        .collect();
    bins.sort_by_key(|b| std::cmp::Reverse(b.0));
    let mut palette: Vec<[f32; 3]> = Vec::new();
    let min_share = (opaque / 200).max(1);
    for (n, colour) in bins {
        if palette.len() >= max_colors || (n < min_share && !palette.is_empty()) {
            break;
        }
        let distinct = palette.iter().all(|p| rgb_distance(p, &colour) > 0.08);
        let blend = palette
            .iter()
            .enumerate()
            .any(|(i, a)| palette[i + 1..].iter().any(|b| is_between(&colour, a, b)));
        if distinct && !blend {
            palette.push(colour);
        }
    }
    let mut out = img.clone();
    for p in out.pixels_mut() {
        if p[3] < 128 {
            *p = image::Rgba([0, 0, 0, 0]);
            continue;
        }
        let nearest = palette
            .iter()
            .min_by(|a, b| {
                let d = |q: &&[f32; 3]| {
                    (0..3)
                        .map(|c| (q[c] - f32::from(p[c])).powi(2))
                        .sum::<f32>()
                };
                d(a).total_cmp(&d(b))
            })
            .copied()
            .unwrap_or([f32::from(p[0]), f32::from(p[1]), f32::from(p[2])]);
        *p = image::Rgba([nearest[0] as u8, nearest[1] as u8, nearest[2] as u8, 255]);
    }
    out
}

fn traced(raw: &str, black: bool, disjoint: bool, cfg: &RenderCfg) -> anyhow::Result<String> {
    let clean = vector::clean_tree(&vector::parse(raw.as_bytes())?, black, disjoint)?;
    ensure!(
        clean.svg.len() <= cfg.max_output_svg_bytes,
        "traced svg too large"
    );
    Ok(clean.svg)
}

fn trace_color(img: &RgbaImage, cfg: &RenderCfg) -> anyhow::Result<String> {
    ensure!(
        img.pixels().any(|p| p[3] >= 128),
        "image is fully transparent"
    );
    let reference = reference(img);
    let work = quantize(&resize_premultiplied(img, WORK_PX), MAX_PALETTE);
    let mut best: Option<(f32, String)> = None;
    for p in [trace::BALANCED, trace::FINE] {
        let Ok(svg) = traced(&trace::color(&work, &p), false, false, cfg) else {
            continue;
        };
        let d = fidelity(&reference, &svg)?;
        let better = match &best {
            None => true,
            Some((bd, bs)) => d + 0.004 < *bd || (d <= *bd + 0.004 && svg.len() < bs.len()),
        };
        if better {
            best = Some((d, svg));
        }
    }
    match best {
        Some((_, svg)) => Ok(svg),
        None => traced(&trace::color(&work, &trace::COARSE), false, false, cfg),
    }
}

fn trace_mask(mask: &RgbaImage, cfg: &RenderCfg) -> anyhow::Result<String> {
    let work = resize_premultiplied(mask, WORK_PX);
    traced(&trace::binary(&work, &trace::BALANCED), true, true, cfg)
        .or_else(|_| traced(&trace::binary(&work, &trace::COARSE), true, true, cfg))
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

fn white_ground(img: &RgbaImage) -> Option<Vec<u8>> {
    let total = img.pixels().len();
    let opaque = img.pixels().filter(|p| p[3] >= 200).count();
    let white = img
        .pixels()
        .filter(|p| p[3] >= 200 && p[0] > 235 && p[1] > 235 && p[2] > 235)
        .count();
    if opaque * 10 < total * 9 || white * 100 < total * 3 || white * 2 > total {
        return None;
    }
    Some(
        img.pixels()
            .map(|p| {
                let soft = smoothstep(0.10, 0.30, color_distance(p, [255.0, 255.0, 255.0]));
                (soft * f32::from(p[3])).round() as u8
            })
            .collect(),
    )
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
            let opaque: Vec<&image::Rgba<u8>> = border.iter().filter(|p| p[3] >= 200).collect();
            let tile_bg = (transparent * 2 < border.len() && !opaque.is_empty())
                .then(|| {
                    let median = |c: usize| {
                        let mut v: Vec<u8> = opaque.iter().map(|p| p[c]).collect();
                        v.sort_unstable();
                        f32::from(v[v.len() / 2])
                    };
                    [median(0), median(1), median(2)]
                })
                .filter(|bg| {
                    let near = opaque
                        .iter()
                        .filter(|p| color_distance(p, *bg) < 0.12)
                        .count();
                    near * 10 >= opaque.len() * 8
                });
            match tile_bg {
                Some(bg) => img
                    .pixels()
                    .map(|p| {
                        let soft = smoothstep(0.10, 0.30, color_distance(p, bg));
                        (soft * f32::from(p[3])).round() as u8
                    })
                    .collect(),
                None => white_ground(img).unwrap_or_else(|| knockout_or_silhouette(img)),
            }
        }
    };
    let coverage = ink.iter().map(|a| u64::from(*a)).sum::<u64>() as f64
        / (255.0 * f64::from(w) * f64::from(h));
    ensure!(coverage >= 0.01, "no visible shape for a monochrome icon");
    ensure!(
        coverage <= 0.99,
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

#[cfg(test)]
pub fn encode_png(img: &RgbaImage) -> anyhow::Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out).write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RenderCfg {
        RenderCfg::default()
    }

    fn paths(svg: &str) -> usize {
        svg.matches("<path").count()
    }

    fn logo(w: u32) -> RgbaImage {
        RgbaImage::from_fn(w, w, |x, y| {
            let c = (w / 2) as i32;
            let d = (x as i32 - c).pow(2) + (y as i32 - c).pow(2);
            let r = (w as i32 * 3 / 8).pow(2);
            if d < r / 4 {
                image::Rgba([255, 255, 255, 255])
            } else if d < r {
                image::Rgba([30, 120, 220, 255])
            } else {
                image::Rgba([0, 0, 0, 0])
            }
        })
    }

    #[test]
    fn traces_raster_into_one_path_per_colour() {
        let v = vectorize(&encode_png(&logo(200)).unwrap(), &cfg()).unwrap();
        assert_eq!(v.origin, VectorOrigin::Traced);
        vector::verify(&v.svg, cfg().max_output_svg_bytes).unwrap();
        assert!(paths(&v.svg) <= 2, "{}", v.svg);
        assert!(
            v.svg.contains("#1e78dc") || v.svg.contains("#1e78dd") || v.svg.contains("#1f78dc"),
            "{}",
            v.svg
        );
        let back = vector::render(&vector::parse(v.svg.as_bytes()).unwrap(), VIEW).unwrap();
        assert_eq!(back.get_pixel(1, 64)[3], 255, "logo is scaled to the edges");
        assert_eq!(back.get_pixel(64, 1)[3], 255, "logo is scaled to the edges");
        assert!(back.get_pixel(64, 64).0.iter().all(|c| *c >= 250));
        assert_eq!(back.get_pixel(2, 2)[3], 0);
    }

    #[test]
    fn traces_tiny_favicons_too() {
        let v = vectorize(&encode_png(&logo(16)).unwrap(), &cfg()).unwrap();
        vector::verify(&v.svg, cfg().max_output_svg_bytes).unwrap();
        assert!(paths(&v.svg) >= 1);
    }

    #[test]
    fn keeps_clean_original_svg_with_gradient() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><defs><radialGradient id="r"><stop offset="0" stop-color="#ffcc00"/><stop offset="1" stop-color="#ff3300"/></radialGradient></defs><circle cx="12" cy="12" r="10" fill="url(#r)"/><path d="M8 12h8" stroke="#ffffff" stroke-width="2" stroke-linecap="round"/></svg>"##;
        let v = vectorize(svg, &cfg()).unwrap();
        assert_eq!(v.origin, VectorOrigin::Original);
        vector::verify(&v.svg, cfg().max_output_svg_bytes).unwrap();
        assert!(v.svg.contains("<radialGradient"));
    }

    #[test]
    fn falls_back_to_tracing_when_sanitizing_loses_too_much() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><defs><pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse"><rect width="1" height="2" fill="#0000ff"/></pattern></defs><rect width="10" height="10" fill="url(#p)"/></svg>"##;
        let v = vectorize(svg, &cfg()).unwrap();
        assert_eq!(v.origin, VectorOrigin::Traced);
        vector::verify(&v.svg, cfg().max_output_svg_bytes).unwrap();
        assert!(!v.svg.contains("pattern"));
    }

    #[test]
    fn hostile_svg_comes_out_clean() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10" onload="fetch('//evil')"><script>alert(1)</script><style>@media screen { circle { fill: #00ff00 } }</style><image href="data:image/svg+xml;base64,PHN2Zz48c2NyaXB0PmFsZXJ0KDEpPC9zY3JpcHQ+PC9zdmc+" width="10" height="10"/><circle cx="5" cy="5" r="4" fill="#ff0000"/></svg>"##;
        let v = vectorize(svg, &cfg()).unwrap();
        vector::verify(&v.svg, cfg().max_output_svg_bytes).unwrap();
        for bad in [
            "script", "style", "image", "onload", "base64", "evil", "@media",
        ] {
            assert!(!v.svg.contains(bad), "{bad} leaked");
        }
    }

    #[test]
    fn mono_is_a_single_black_path() {
        let v = vectorize_mono(
            &encode_png(&logo(128)).unwrap(),
            &cfg(),
            MonoSource::Derived,
        )
        .unwrap();
        vector::verify(&v.svg, cfg().max_output_svg_bytes).unwrap();
        assert_eq!(paths(&v.svg), 1, "{}", v.svg);
        assert!(v.svg.contains("#000000"));
        let mask = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M2 2h12v12H2z" fill="#555"/></svg>"##;
        let m = vectorize_mono(mask, &cfg(), MonoSource::MaskIcon).unwrap();
        assert_eq!(m.origin, VectorOrigin::Original);
        assert!(m.svg.contains("#000000") && !m.svg.contains("#555"));
    }

    #[test]
    fn drops_antialiasing_blends_from_palette() {
        let img = RgbaImage::from_fn(60, 20, |x, _| match x {
            0..=24 => image::Rgba([255, 255, 255, 255]),
            25..=34 => image::Rgba([245, 160, 153, 255]),
            _ => image::Rgba([235, 66, 52, 255]),
        });
        let q = quantize(&img, 12);
        let colours: std::collections::HashSet<[u8; 4]> = q.pixels().map(|p| p.0).collect();
        assert_eq!(colours.len(), 2, "{colours:?}");
    }

    #[test]
    fn rejects_fully_transparent_images() {
        assert!(vectorize(&encode_png(&RgbaImage::new(64, 64)).unwrap(), &cfg()).is_err());
    }

    fn png_of(w: u32, h: u32) -> Vec<u8> {
        encode_png(&RgbaImage::from_pixel(
            w,
            h,
            image::Rgba([200, 10, 10, 255]),
        ))
        .unwrap()
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
    fn multicolour_edges_are_not_a_tile() {
        let colours = [[242, 80, 34], [127, 186, 0], [0, 164, 239], [255, 185, 0]];
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            if x % 32 >= 30 || y % 32 >= 30 {
                image::Rgba([0, 0, 0, 0])
            } else {
                let c = colours[((y / 32) * 2 + x / 32) as usize];
                image::Rgba([c[0], c[1], c[2], 255])
            }
        });
        let m = monochrome(&img, MonoSource::Derived).unwrap();
        for (x, y) in [(10, 10), (50, 10), (10, 50), (50, 50)] {
            assert_eq!(alpha_at(&m, x, y), 255, "square at {x},{y} must stay");
        }
    }

    #[test]
    fn opaque_logo_on_white_cross_keeps_its_squares() {
        let colours = [[242, 80, 34], [127, 186, 0], [0, 164, 239], [255, 185, 0]];
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            if (30..34).contains(&x) || (30..34).contains(&y) {
                image::Rgba([253, 254, 252, 255])
            } else {
                let c = colours[((y / 32) * 2 + x / 32) as usize];
                image::Rgba([c[0], c[1], c[2], 255])
            }
        });
        let m = monochrome(&img, MonoSource::Derived).unwrap();
        assert_eq!(alpha_at(&m, 10, 10), 255);
        assert_eq!(alpha_at(&m, 50, 50), 255);
        assert_eq!(alpha_at(&m, 32, 10), 0);
    }

    #[test]
    fn rejects_shapeless_monochrome() {
        let flat = RgbaImage::from_pixel(32, 32, image::Rgba([10, 200, 10, 255]));
        assert!(monochrome(&flat, MonoSource::Derived).is_err());
        assert!(monochrome(&RgbaImage::new(32, 32), MonoSource::MaskIcon).is_err());
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
        assert!(vectorize(data, &cfg()).is_err(), "{name} must be rejected");
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
    fn svg_cannot_reach_filesystem_or_network() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10">
            <image href="/etc/passwd" width="10" height="10"/>
            <image xlink:href="https://169.254.169.254/latest/meta-data" width="10" height="10"/>
            <rect width="10" height="10" fill="#0f0"/></svg>"##;
        assert!(vectorize(svg, &cfg()).is_ok());
    }

    #[test]
    fn rejects_entity_expansion_and_deep_nesting() {
        let lol = br#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY a "aaaaaaaa"><!ENTITY b "&a;&a;&a;&a;">]><svg xmlns="http://www.w3.org/2000/svg">&b;</svg>"#;
        assert!(vectorize(lol, &cfg()).is_err());
        let deep = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg">{}{}</svg>"#,
            "<g>".repeat(5000),
            "</g>".repeat(5000)
        );
        assert!(vectorize(deep.as_bytes(), &cfg()).is_err());
    }

    #[test]
    fn measures_markup_depth() {
        assert_eq!(markup_depth(b"<a><b/><c><d></d></c></a>"), 3);
        assert_eq!(markup_depth(b"<?xml?><!-- <x><y> --><a></a>"), 1);
    }
}
