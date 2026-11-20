use std::fmt;

use http::{HeaderName, HeaderValue};
use image::RgbaImage;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::contract::{
    H_ACCENT, H_DARK_MONO, H_DARK_MONO_PLATE, H_DARK_PLATE, H_DARK_TEXT, H_FILL, H_LIGHT_MONO,
    H_LIGHT_MONO_PLATE, H_LIGHT_PLATE, H_LIGHT_TEXT, H_SHAPE,
};
use crate::vector;

pub const SIDE: u32 = 64;
pub const LIGHT_BACKGROUND: Rgb = Rgb([0xff, 0xff, 0xff]);
pub const DARK_BACKGROUND: Rgb = Rgb([0x1c, 0x1c, 0x1e]);
const TEXT_CONTRAST: f64 = 4.5;
const GRAPHIC_CONTRAST: f64 = 3.0;
const VISIBLE_CONTRAST: f64 = 1.6;
const HOLE_CONTRAST: f64 = 2.0;
const OPAQUE: u8 = 200;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    Square,
    Rounded,
    Circle,
    None,
}

impl Shape {
    pub fn as_str(self) -> &'static str {
        match self {
            Shape::Square => "square",
            Shape::Rounded => "rounded",
            Shape::Circle => "circle",
            Shape::None => "none",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fill {
    Bleed,
    #[default]
    Inset,
}

impl Fill {
    pub fn as_str(self) -> &'static str {
        match self {
            Fill::Bleed => "bleed",
            Fill::Inset => "inset",
        }
    }
}

const BLEED_MAX_RADIUS: f64 = 0.25;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}")
    }
}

impl fmt::Debug for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Rgb::parse(&s).ok_or_else(|| serde::de::Error::custom("expected #rrggbb"))
    }
}

impl Rgb {
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.strip_prefix('#')?;
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Rgb([byte(0)?, byte(2)?, byte(4)?]))
    }

    fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let [r, g, b] = self.0;
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    }

    pub fn contrast(self, other: Rgb) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn hsl(self) -> (f64, f64, f64) {
        let [r, g, b] = self.0.map(|c| f64::from(c) / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let l = (max + min) / 2.0;
        let d = max - min;
        if d < 1e-9 {
            return (0.0, 0.0, l);
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h * 60.0, s.min(1.0), l)
    }

    fn from_hsl(h: f64, s: f64, l: f64) -> Self {
        let a = s * l.min(1.0 - l);
        let f = |n: f64| {
            let k = (n + h / 30.0) % 12.0;
            let v = l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0);
            (v.clamp(0.0, 1.0) * 255.0).round() as u8
        };
        Rgb([f(0.0), f(8.0), f(4.0)])
    }

    fn with_lightness(self, l: f64, max_saturation: f64) -> Self {
        let (h, s, _) = self.hsl();
        Rgb::from_hsl(h, s.min(max_saturation), l)
    }

    fn over(self, alpha: f64, base: Rgb) -> Rgb {
        let mix =
            |a: u8, b: u8| (f64::from(a) * alpha + f64::from(b) * (1.0 - alpha)).round() as u8;
        Rgb([
            mix(self.0[0], base.0[0]),
            mix(self.0[1], base.0[1]),
            mix(self.0[2], base.0[2]),
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scheme {
    pub plate: Rgb,
    pub text: Rgb,
    pub mono: Rgb,
    pub mono_plate: Rgb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub shape: Shape,
    #[serde(default)]
    pub fill: Fill,
    pub accent: Rgb,
    pub light: Scheme,
    pub dark: Scheme,
}

pub type Headers = Vec<(HeaderName, HeaderValue)>;

impl Palette {
    pub fn headers(&self) -> Headers {
        let colour = |name: &'static str, c: Rgb| {
            HeaderValue::from_str(&c.to_string())
                .ok()
                .map(|v| (HeaderName::from_static(name), v))
        };
        let mut out = vec![
            (
                HeaderName::from_static(H_SHAPE),
                HeaderValue::from_static(self.shape.as_str()),
            ),
            (
                HeaderName::from_static(H_FILL),
                HeaderValue::from_static(self.fill.as_str()),
            ),
        ];
        out.extend(
            [
                colour(H_ACCENT, self.accent),
                colour(H_LIGHT_PLATE, self.light.plate),
                colour(H_LIGHT_TEXT, self.light.text),
                colour(H_LIGHT_MONO, self.light.mono),
                colour(H_LIGHT_MONO_PLATE, self.light.mono_plate),
                colour(H_DARK_PLATE, self.dark.plate),
                colour(H_DARK_TEXT, self.dark.text),
                colour(H_DARK_MONO, self.dark.mono),
                colour(H_DARK_MONO_PLATE, self.dark.mono_plate),
            ]
            .into_iter()
            .flatten(),
        );
        out
    }
}

pub fn analyze_svg(svg: &str) -> anyhow::Result<Palette> {
    let tree = vector::parse(svg.as_bytes())?;
    Ok(analyze(&vector::render(&tree, SIDE)?))
}

struct Pixels {
    side: u32,
    px: Vec<(Rgb, f64)>,
}

impl Pixels {
    fn new(img: &RgbaImage) -> Self {
        let px = img
            .pixels()
            .map(|p| (Rgb([p[0], p[1], p[2]]), f64::from(p[3]) / 255.0))
            .collect();
        Self {
            side: img.width().min(img.height()),
            px,
        }
    }

    fn at(&self, x: u32, y: u32) -> (Rgb, f64) {
        self.px[(y * self.side + x) as usize]
    }

    fn opaque(&self, x: u32, y: u32) -> bool {
        self.at(x, y).1 >= f64::from(OPAQUE) / 255.0
    }
}

pub fn analyze(img: &RgbaImage) -> Palette {
    let p = Pixels::new(img);
    let (shape, tile) = shape(&p);
    let fill = match shape {
        Shape::Square => Fill::Bleed,
        Shape::Rounded if corner_radius(&p) <= BLEED_MAX_RADIUS => Fill::Bleed,
        _ => Fill::Inset,
    };
    let accent = accent(&p);
    let holes = tile.is_some() && has_holes(&p, shape);
    let (_, saturation, _) = accent.hsl();
    let neutral = saturation < 0.12;
    let scheme = |background: Rgb, preferred: [Rgb; 4]| {
        let dark = background.luminance() < 0.18;
        let mono_plate = if dark {
            accent.with_lightness(0.19, 0.4)
        } else {
            accent.with_lightness(0.94, 0.55)
        };
        let (text, mono) = if neutral && dark {
            (Rgb([0xe8, 0xe8, 0xea]), Rgb([0xd4, 0xd4, 0xd8]))
        } else {
            (
                readable(accent, background, TEXT_CONTRAST),
                readable(accent, mono_plate, GRAPHIC_CONTRAST),
            )
        };
        Scheme {
            plate: pick_plate(&p, tile, holes, preferred),
            text,
            mono,
            mono_plate,
        }
    };
    let soft_light = accent.with_lightness(0.95, 0.6);
    let soft_dark = accent.with_lightness(0.17, 0.45);
    Palette {
        shape,
        fill,
        accent,
        light: scheme(
            LIGHT_BACKGROUND,
            [soft_light, LIGHT_BACKGROUND, DARK_BACKGROUND, soft_dark],
        ),
        dark: scheme(
            DARK_BACKGROUND,
            [
                soft_dark,
                Rgb([0x2c, 0x2c, 0x2e]),
                LIGHT_BACKGROUND,
                soft_light,
            ],
        ),
    }
}

fn coverage(p: &Pixels, points: impl Iterator<Item = (u32, u32)>) -> f64 {
    let (mut hit, mut total) = (0u32, 0u32);
    for (x, y) in points {
        total += 1;
        hit += u32::from(p.opaque(x, y));
    }
    if total == 0 {
        0.0
    } else {
        f64::from(hit) / f64::from(total)
    }
}

fn edge(p: &Pixels, from: f64, to: f64) -> f64 {
    let n = p.side;
    let (a, b) = ((from * f64::from(n)) as u32, (to * f64::from(n)) as u32);
    let inset = 1;
    let far = n - 1 - inset;
    let sides = [
        coverage(p, (a..b).map(|i| (i, inset))),
        coverage(p, (a..b).map(|i| (i, far))),
        coverage(p, (a..b).map(|i| (inset, i))),
        coverage(p, (a..b).map(|i| (far, i))),
    ];
    sides.into_iter().fold(1.0, f64::min)
}

fn corners(p: &Pixels) -> f64 {
    let n = p.side - 2;
    coverage(p, [(1, 1), (n, 1), (1, n), (n, n)].into_iter())
}

fn ring(p: &Pixels, radius: f64) -> f64 {
    let c = f64::from(p.side) / 2.0;
    coverage(
        p,
        (0..48).map(|i| {
            let t = f64::from(i) / 48.0 * std::f64::consts::TAU;
            let x = (c + radius * t.cos()).clamp(0.0, f64::from(p.side - 1));
            let y = (c + radius * t.sin()).clamp(0.0, f64::from(p.side - 1));
            (x as u32, y as u32)
        }),
    )
}

fn shape(p: &Pixels) -> (Shape, Option<Rgb>) {
    let n = f64::from(p.side);
    let shape = if corners(p) >= 0.99 && edge(p, 0.0, 1.0) >= 0.95 {
        Shape::Square
    } else if corners(p) <= 0.01 && edge(p, 0.2, 0.8) >= 0.95 {
        Shape::Rounded
    } else if corners(p) <= 0.01 && edge(p, 0.45, 0.55) >= 0.9 && ring(p, n * 0.46) >= 0.95 {
        Shape::Circle
    } else {
        Shape::None
    };
    if shape == Shape::None {
        return (shape, None);
    }
    let inset = 2;
    let far = p.side - 1 - inset;
    let (a, b) = ((n * 0.3) as u32, (n * 0.7) as u32);
    let samples: Vec<Rgb> = (a..b)
        .flat_map(|i| [(i, inset), (i, far), (inset, i), (far, i)])
        .filter(|&(x, y)| p.opaque(x, y))
        .map(|(x, y)| p.at(x, y).0)
        .collect();
    (shape, median(&samples))
}

fn corner_radius(p: &Pixels) -> f64 {
    let last = p.side - 1;
    let gaps: Vec<f64> = [
        (0, 0, 1, 1),
        (last, 0, -1, 1),
        (0, last, 1, -1),
        (last, last, -1, -1),
    ]
    .into_iter()
    .map(|(x0, y0, dx, dy): (u32, u32, i32, i32)| {
        let steps = (0..p.side / 2)
            .find(|&i| {
                let x = x0.saturating_add_signed(dx * i as i32);
                let y = y0.saturating_add_signed(dy * i as i32);
                p.opaque(x, y)
            })
            .unwrap_or(p.side / 2);
        f64::from(steps) + 0.5
    })
    .collect();
    let gap = gaps.iter().sum::<f64>() / gaps.len() as f64;
    gap * std::f64::consts::SQRT_2 / (std::f64::consts::SQRT_2 - 1.0) / f64::from(p.side)
}

fn median(colors: &[Rgb]) -> Option<Rgb> {
    if colors.is_empty() {
        return None;
    }
    let mid = |c: usize| {
        let mut v: Vec<u8> = colors.iter().map(|x| x.0[c]).collect();
        v.sort_unstable();
        v[v.len() / 2]
    };
    Some(Rgb([mid(0), mid(1), mid(2)]))
}

fn inside(shape: Shape, side: u32, x: u32, y: u32) -> bool {
    let n = f64::from(side);
    let (fx, fy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
    match shape {
        Shape::Circle => {
            let c = n / 2.0;
            (fx - c).powi(2) + (fy - c).powi(2) <= (n * 0.42).powi(2)
        }
        Shape::Square | Shape::Rounded => {
            let m = n * 0.12;
            fx > m && fy > m && fx < n - m && fy < n - m
        }
        Shape::None => false,
    }
}

fn has_holes(p: &Pixels, shape: Shape) -> bool {
    let (mut holes, mut total) = (0u32, 0u32);
    for y in 0..p.side {
        for x in 0..p.side {
            if inside(shape, p.side, x, y) {
                total += 1;
                holes += u32::from(p.at(x, y).1 < 0.5);
            }
        }
    }
    total > 0 && f64::from(holes) / f64::from(total) > 0.02
}

fn accent(p: &Pixels) -> Rgb {
    let mut bins = [(0.0f64, [0.0f64; 3], 0.0f64); 24];
    let mut grey: Vec<Rgb> = Vec::new();
    for &(c, a) in &p.px {
        if a < 0.5 {
            continue;
        }
        let (h, s, l) = c.hsl();
        if s >= 0.25 && (0.12..=0.88).contains(&l) {
            let bin = &mut bins[((h / 15.0) as usize).min(23)];
            let w = a * s;
            bin.0 += w;
            for (i, v) in c.0.iter().enumerate() {
                bin.1[i] += f64::from(*v) * w;
            }
            bin.2 += a;
        } else {
            grey.push(c);
        }
    }
    let opaque: f64 = p.px.iter().filter(|(_, a)| *a >= 0.5).count() as f64;
    if let Some(best) = bins
        .iter()
        .filter(|b| b.2 >= opaque * 0.04)
        .max_by(|a, b| a.0.total_cmp(&b.0))
    {
        return Rgb(best.1.map(|v| (v / best.0).round().clamp(0.0, 255.0) as u8));
    }
    let darkest = grey
        .iter()
        .filter(|c| c.hsl().2 < 0.9)
        .copied()
        .collect::<Vec<_>>();
    median(&darkest)
        .or_else(|| median(&grey))
        .unwrap_or(Rgb([0x6b, 0x6b, 0x73]))
}

fn visible(p: &Pixels, plate: Rgb) -> f64 {
    let (mut seen, mut total) = (0.0, 0.0);
    for &(c, a) in &p.px {
        if a < 0.25 {
            continue;
        }
        total += a;
        if c.over(a, plate).contrast(plate) >= VISIBLE_CONTRAST {
            seen += a;
        }
    }
    if total == 0.0 { 1.0 } else { seen / total }
}

fn pick_plate(p: &Pixels, tile: Option<Rgb>, holes: bool, preferred: [Rgb; 4]) -> Rgb {
    if let Some(tile) = tile {
        if !holes {
            return preferred[0];
        }
        return preferred
            .into_iter()
            .find(|c| c.contrast(tile) >= HOLE_CONTRAST)
            .unwrap_or_else(|| {
                preferred
                    .into_iter()
                    .max_by(|a, b| a.contrast(tile).total_cmp(&b.contrast(tile)))
                    .unwrap_or(preferred[0])
            });
    }
    let scores = preferred.map(|c| visible(p, c));
    let best = scores.iter().copied().fold(0.0, f64::max);
    preferred
        .into_iter()
        .zip(scores)
        .find(|&(_, s)| s >= 0.6 || s >= best - 0.05)
        .map_or(preferred[0], |(c, _)| c)
}

fn readable(accent: Rgb, plate: Rgb, target: f64) -> Rgb {
    if accent.contrast(plate) >= target {
        return accent;
    }
    let (h, s, l) = accent.hsl();
    let darker = plate.luminance() > 0.18;
    let (mut lo, mut hi) = if darker { (0.0, l) } else { (l, 1.0) };
    for _ in 0..24 {
        let mid = (lo + hi) / 2.0;
        let ok = Rgb::from_hsl(h, s, mid).contrast(plate) >= target;
        if ok == darker {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let l = if darker { lo } else { hi };
    let c = Rgb::from_hsl(h, s, l);
    if c.contrast(plate) >= target {
        c
    } else if darker {
        Rgb([0, 0, 0])
    } else {
        Rgb([0xff, 0xff, 0xff])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128">"#;

    fn palette(body: &str) -> Palette {
        analyze_svg(&format!("{PRE}{body}</svg>")).unwrap()
    }

    #[test]
    fn detects_shapes() {
        assert_eq!(
            palette(r##"<rect width="128" height="128" fill="#1d4ed8"/>"##).shape,
            Shape::Square
        );
        assert_eq!(
            palette(r##"<rect width="128" height="128" rx="28" fill="#1d4ed8"/>"##).shape,
            Shape::Rounded
        );
        assert_eq!(
            palette(r##"<circle cx="64" cy="64" r="64" fill="#1db954"/>"##).shape,
            Shape::Circle
        );
        assert_eq!(
            palette(r##"<path d="M64 0L128 128H0z" fill="#e11d48"/>"##).shape,
            Shape::None
        );
    }

    #[test]
    fn full_tiles_bleed_and_everything_else_is_inset() {
        let fill = |body: &str| palette(body).fill;
        assert_eq!(
            fill(r##"<rect width="128" height="128" fill="#1d4ed8"/>"##),
            Fill::Bleed
        );
        assert_eq!(
            fill(r##"<rect width="128" height="128" rx="6" fill="#1d4ed8"/>"##),
            Fill::Bleed
        );
        assert_eq!(
            fill(r##"<rect width="128" height="128" rx="26" fill="#1d4ed8"/>"##),
            Fill::Bleed
        );
        assert_eq!(
            fill(r##"<rect width="128" height="128" rx="44" fill="#1d4ed8"/>"##),
            Fill::Inset
        );
        assert_eq!(
            fill(r##"<circle cx="64" cy="64" r="64" fill="#1db954"/>"##),
            Fill::Inset
        );
        assert_eq!(
            fill(r##"<path d="M64 0L128 128H0z" fill="#e11d48"/>"##),
            Fill::Inset
        );
        assert_eq!(
            fill(
                r##"<path fill="#000000" fill-rule="evenodd" d="M0 0H128V128H0zM40 40H88V88H40z"/>"##
            ),
            Fill::Bleed
        );
    }

    #[test]
    fn cutout_tile_never_gets_a_plate_of_its_own_colour() {
        let p = palette(
            r##"<path fill="#000000" fill-rule="evenodd" d="M0 0H128V128H0zM40 40H88V88H40z"/>"##,
        );
        assert_eq!(p.shape, Shape::Square);
        for scheme in [p.light, p.dark] {
            assert!(
                scheme.plate.contrast(Rgb([0, 0, 0])) >= HOLE_CONTRAST,
                "{scheme:?}"
            );
        }
    }

    #[test]
    fn dark_glyph_gets_a_light_plate_in_dark_theme() {
        let p = palette(r##"<path d="M64 8L120 120H8z" fill="#111111"/>"##);
        assert!(p.dark.plate.contrast(Rgb([0x11, 0x11, 0x11])) >= VISIBLE_CONTRAST);
        assert!(p.light.plate.contrast(Rgb([0x11, 0x11, 0x11])) >= 3.0);
    }

    #[test]
    fn white_glyph_gets_a_dark_plate_in_light_theme() {
        let p = palette(r##"<path d="M64 8L120 120H8z" fill="#ffffff"/>"##);
        assert!(p.light.plate.contrast(Rgb([0xff, 0xff, 0xff])) >= 3.0);
    }

    #[test]
    fn picks_brand_accent_and_readable_text() {
        let p = palette(
            r##"<circle cx="64" cy="64" r="64" fill="#1db954"/><path d="M30 50H98V62H30z" fill="#000000"/>"##,
        );
        let (h, s, _) = p.accent.hsl();
        assert!((120.0..160.0).contains(&h) && s > 0.4, "{:?}", p.accent);
        for (scheme, bg) in [(p.light, LIGHT_BACKGROUND), (p.dark, DARK_BACKGROUND)] {
            assert!(scheme.text.contrast(bg) >= TEXT_CONTRAST, "{scheme:?}");
            assert!(scheme.mono.contrast(bg) >= GRAPHIC_CONTRAST, "{scheme:?}");
        }
        for scheme in [p.light, p.dark] {
            assert!(
                scheme.mono.contrast(scheme.mono_plate) >= GRAPHIC_CONTRAST,
                "{scheme:?}"
            );
            let (h, _, _) = scheme.mono_plate.hsl();
            let (mh, _, _) = scheme.mono.hsl();
            assert!((h - mh).abs() < 12.0, "{scheme:?}");
        }
        let black = palette(r##"<path d="M64 8L120 120H8z" fill="#181717"/>"##);
        for scheme in [black.light, black.dark] {
            assert!(
                scheme.mono.contrast(scheme.mono_plate) >= GRAPHIC_CONTRAST,
                "{scheme:?}"
            );
        }
        assert!(black.dark.text.contrast(DARK_BACKGROUND) >= TEXT_CONTRAST);
        assert!(black.dark.mono.contrast(DARK_BACKGROUND) >= GRAPHIC_CONTRAST);
    }

    #[test]
    fn colours_round_trip_as_hex() {
        let c = Rgb([0x1d, 0xb9, 0x54]);
        assert_eq!(c.to_string(), "#1db954");
        assert_eq!(Rgb::parse("#1db954"), Some(c));
        assert_eq!(Rgb::parse("#1db95"), None);
        assert_eq!(Rgb::parse("1db954"), None);
        assert_eq!(Rgb::parse("#1db95g"), None);
    }
}
