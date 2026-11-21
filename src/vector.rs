use std::collections::HashMap;
use std::fmt::Write;

use anyhow::{Context, bail, ensure};
use image::RgbaImage;
use resvg::{tiny_skia, usvg};
use serde::{Deserialize, Serialize};
use usvg::tiny_skia_path::{self as tsp, PathSegment, Transform};

pub const VIEW: u32 = 128;
const MAX_SHAPES: usize = 2048;
const SVG_PREFIX: &str =
    r#"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VectorOrigin {
    Original,
    Traced,
}

impl VectorOrigin {
    pub const ALL: [VectorOrigin; 2] = [VectorOrigin::Original, VectorOrigin::Traced];

    pub fn as_str(self) -> &'static str {
        match self {
            VectorOrigin::Original => "original",
            VectorOrigin::Traced => "traced",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Vector {
    pub svg: String,
    pub origin: VectorOrigin,
}

#[derive(Debug)]
pub struct Clean {
    pub svg: String,
    pub lossy: bool,
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum PaintKey {
    Solid([u8; 3], u8),
    Gradient(usize),
}

struct Shape {
    path: tsp::Path,
    paint: PaintKey,
    evenodd: bool,
}

struct Builder {
    view: Transform,
    black: bool,
    lossy: bool,
    defs: Vec<Gradient>,
    def_index: HashMap<String, usize>,
    shapes: Vec<Shape>,
}

struct Gradient {
    head: String,
    transform: Transform,
    tail: String,
}

impl Gradient {
    fn render(&self, id: usize, extra: Transform) -> String {
        let t = extra.pre_concat(self.transform);
        format!(
            r#"{} id="g{id}" gradientTransform="matrix({} {} {} {} {} {})"{}"#,
            self.head,
            num(t.sx, 5),
            num(t.ky, 5),
            num(t.kx, 5),
            num(t.sy, 5),
            num(t.tx, 3),
            num(t.ty, 3),
            self.tail
        )
    }
}

pub fn view_transform(width: f32, height: f32) -> Transform {
    let side = width.max(height);
    let s = VIEW as f32 / side;
    Transform::from_row(
        s,
        0.0,
        0.0,
        s,
        (VIEW as f32 - width * s) / 2.0,
        (VIEW as f32 - height * s) / 2.0,
    )
}

pub fn clean_tree(tree: &usvg::Tree, black: bool, disjoint: bool) -> anyhow::Result<Clean> {
    build(tree, black, disjoint, false)
}

pub fn fit(svg: &str) -> anyhow::Result<String> {
    Ok(build(&parse(svg.as_bytes())?, false, false, true)?.svg)
}

fn is_backdrop(s: &Shape) -> bool {
    let PaintKey::Solid(rgb, a) = s.paint else {
        return false;
    };
    let b = s.path.compute_tight_bounds().unwrap_or(s.path.bounds());
    let full = VIEW as f32 * 0.97;
    a >= 250 && rgb.iter().all(|c| *c >= 235) && b.width() >= full && b.height() >= full
}

fn is_light(s: &Shape) -> bool {
    matches!(s.paint, PaintKey::Solid(rgb, _) if rgb.iter().all(|c| *c >= 235))
}

fn fit_shapes(shapes: &mut Vec<Shape>) -> Transform {
    if shapes.len() > 1 && is_backdrop(&shapes[0]) && shapes[1..].iter().any(|s| !is_light(s)) {
        shapes.remove(0);
    }
    let bounds = shapes
        .iter()
        .filter_map(|s| s.path.compute_tight_bounds())
        .reduce(|a, b| {
            tsp::Rect::from_ltrb(
                a.left().min(b.left()),
                a.top().min(b.top()),
                a.right().max(b.right()),
                a.bottom().max(b.bottom()),
            )
            .unwrap_or(a)
        });
    let Some(b) = bounds.filter(|b| b.width() > 0.5 && b.height() > 0.5) else {
        return Transform::identity();
    };
    let view = VIEW as f32;
    let scale = view / b.width().max(b.height());
    let t = Transform::from_row(
        scale,
        0.0,
        0.0,
        scale,
        (view - b.width() * scale) / 2.0 - b.left() * scale,
        (view - b.height() * scale) / 2.0 - b.top() * scale,
    );
    for s in shapes.iter_mut() {
        if let Some(p) = s.path.clone().transform(t) {
            s.path = p;
        }
    }
    t
}

fn build(tree: &usvg::Tree, black: bool, disjoint: bool, fill: bool) -> anyhow::Result<Clean> {
    let size = tree.size();
    let mut b = Builder {
        view: view_transform(size.width(), size.height()),
        black,
        lossy: false,
        defs: Vec::new(),
        def_index: HashMap::new(),
        shapes: Vec::new(),
    };
    b.walk(tree.root(), 1.0)?;
    let mut shapes = merge(std::mem::take(&mut b.shapes), disjoint);
    let extra = if fill {
        fit_shapes(&mut shapes)
    } else {
        Transform::identity()
    };
    let mut out = String::with_capacity(4096);
    out.push_str(SVG_PREFIX);
    let used: std::collections::HashSet<usize> = shapes
        .iter()
        .filter_map(|s| match s.paint {
            PaintKey::Gradient(i) => Some(i),
            PaintKey::Solid(..) => None,
        })
        .collect();
    if !used.is_empty() {
        out.push_str("<defs>");
        for (i, def) in b.defs.iter().enumerate() {
            if used.contains(&i) {
                out.push_str(&def.render(i, extra));
            }
        }
        out.push_str("</defs>");
    }
    let mut paths = 0;
    for s in &shapes {
        let d = path_data(&s.path);
        if d.is_empty() {
            continue;
        }
        paths += 1;
        out.push_str("<path fill=\"");
        match &s.paint {
            PaintKey::Solid(rgb, a) => {
                let _ = write!(out, "#{:02x}{:02x}{:02x}\"", rgb[0], rgb[1], rgb[2]);
                if *a < 255 {
                    let _ = write!(out, " fill-opacity=\"{}\"", num(f32::from(*a) / 255.0, 3));
                }
            }
            PaintKey::Gradient(i) => {
                let _ = write!(out, "url(#g{i})\"");
            }
        }
        if s.evenodd {
            out.push_str(" fill-rule=\"evenodd\"");
        }
        let _ = write!(out, " d=\"{d}\"/>");
    }
    out.push_str("</svg>");
    ensure!(paths > 0, "nothing visible after sanitizing");
    Ok(Clean {
        svg: out,
        lossy: b.lossy,
    })
}

impl Builder {
    fn walk(&mut self, group: &usvg::Group, opacity: f32) -> anyhow::Result<()> {
        if group.clip_path().is_some() || group.mask().is_some() || !group.filters().is_empty() {
            self.lossy = true;
        }
        let opacity = opacity * group.opacity().get();
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => self.walk(g, opacity)?,
                usvg::Node::Path(p) => self.path(p, opacity)?,
                usvg::Node::Image(_) | usvg::Node::Text(_) => self.lossy = true,
            }
            ensure!(self.shapes.len() <= MAX_SHAPES, "too many shapes");
        }
        Ok(())
    }

    fn path(&mut self, p: &usvg::Path, opacity: f32) -> anyhow::Result<()> {
        if !p.is_visible() {
            return Ok(());
        }
        let tf = self.view.pre_concat(p.abs_transform());
        if let Some(fill) = p.fill()
            && let Some(paint) = self.paint(fill.paint(), fill.opacity().get() * opacity, tf)
            && let Some(path) = p.data().clone().transform(tf)
        {
            self.shapes.push(Shape {
                path,
                paint,
                evenodd: fill.rule() == usvg::FillRule::EvenOdd,
            });
        }
        if let Some(stroke) = p.stroke()
            && let Some(paint) = self.paint(stroke.paint(), stroke.opacity().get() * opacity, tf)
        {
            let mut st = tsp::Stroke {
                width: stroke.width().get(),
                miter_limit: stroke.miterlimit().get(),
                line_cap: match stroke.linecap() {
                    usvg::LineCap::Butt => tsp::LineCap::Butt,
                    usvg::LineCap::Round => tsp::LineCap::Round,
                    usvg::LineCap::Square => tsp::LineCap::Square,
                },
                line_join: match stroke.linejoin() {
                    usvg::LineJoin::Miter => tsp::LineJoin::Miter,
                    usvg::LineJoin::MiterClip => tsp::LineJoin::MiterClip,
                    usvg::LineJoin::Round => tsp::LineJoin::Round,
                    usvg::LineJoin::Bevel => tsp::LineJoin::Bevel,
                },
                dash: None,
            };
            if let Some(dash) = stroke.dasharray() {
                st.dash = tsp::StrokeDash::new(dash.to_vec(), stroke.dashoffset());
            }
            let scale = (tf.sx * tf.sy - tf.kx * tf.ky).abs().sqrt().max(0.01);
            if let Some(outline) = p.data().stroke(&st, scale)
                && let Some(path) = outline.transform(tf)
            {
                self.shapes.push(Shape {
                    path,
                    paint,
                    evenodd: false,
                });
            }
        }
        Ok(())
    }

    fn paint(&mut self, paint: &usvg::Paint, opacity: f32, tf: Transform) -> Option<PaintKey> {
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        if alpha == 0 {
            return None;
        }
        if self.black {
            return Some(PaintKey::Solid([0, 0, 0], alpha));
        }
        let (kind, attrs, base) = match paint {
            usvg::Paint::Color(c) => return Some(PaintKey::Solid([c.red, c.green, c.blue], alpha)),
            usvg::Paint::Pattern(_) => {
                self.lossy = true;
                return None;
            }
            usvg::Paint::LinearGradient(g) => (
                "linearGradient",
                format!(
                    r#" x1="{}" y1="{}" x2="{}" y2="{}""#,
                    num(g.x1(), 3),
                    num(g.y1(), 3),
                    num(g.x2(), 3),
                    num(g.y2(), 3)
                ),
                (g.transform(), g.spread_method(), g.stops()),
            ),
            usvg::Paint::RadialGradient(g) => (
                "radialGradient",
                format!(
                    r#" cx="{}" cy="{}" r="{}" fx="{}" fy="{}""#,
                    num(g.cx(), 3),
                    num(g.cy(), 3),
                    num(g.r().get(), 3),
                    num(g.fx(), 3),
                    num(g.fy(), 3)
                ),
                (g.transform(), g.spread_method(), g.stops()),
            ),
        };
        let (gt, spread, stops) = base;
        let t = tf.pre_concat(gt);
        let spread = match spread {
            usvg::SpreadMethod::Pad => "",
            usvg::SpreadMethod::Reflect => r#" spreadMethod="reflect""#,
            usvg::SpreadMethod::Repeat => r#" spreadMethod="repeat""#,
        };
        let head = format!(r#"<{kind} gradientUnits="userSpaceOnUse"{attrs}"#);
        let mut def = format!("{spread}>");
        for s in stops.iter().take(64) {
            let c = s.color();
            let a = s.opacity().get() * opacity;
            let _ = write!(
                def,
                r##"<stop offset="{}" stop-color="#{:02x}{:02x}{:02x}""##,
                num(s.offset().get(), 4),
                c.red,
                c.green,
                c.blue
            );
            if a < 0.999 {
                let _ = write!(def, r#" stop-opacity="{}""#, num(a, 3));
            }
            def.push_str("/>");
        }
        let _ = write!(def, "</{kind}>");
        let gradient = Gradient {
            head,
            transform: t,
            tail: def,
        };
        let key = gradient.render(0, Transform::identity());
        let next = self.defs.len();
        let index = *self.def_index.entry(key).or_insert(next);
        if index == next {
            self.defs.push(gradient);
        }
        Some(PaintKey::Gradient(index))
    }
}

fn merge(shapes: Vec<Shape>, disjoint: bool) -> Vec<Shape> {
    if disjoint {
        let mut order: Vec<(PaintKey, bool)> = Vec::new();
        let mut groups: HashMap<(PaintKey, bool), tsp::PathBuilder> = HashMap::new();
        for s in shapes {
            let key = (s.paint.clone(), s.evenodd);
            groups
                .entry(key.clone())
                .or_insert_with(|| {
                    order.push(key);
                    tsp::PathBuilder::new()
                })
                .push_path(&s.path);
        }
        return order
            .into_iter()
            .filter_map(|key| {
                let path = groups.remove(&key)?.finish()?;
                Some(Shape {
                    path,
                    paint: key.0,
                    evenodd: key.1,
                })
            })
            .collect();
    }
    let mut out: Vec<Shape> = Vec::new();
    for s in shapes {
        let bounds = s.path.bounds();
        let mut target = None;
        for (i, prev) in out.iter().enumerate().rev() {
            if prev.paint == s.paint && prev.evenodd == s.evenodd {
                if prev.path.bounds().intersect(&bounds).is_none() {
                    target = Some(i);
                }
                break;
            }
            if prev.path.bounds().intersect(&bounds).is_some() {
                break;
            }
        }
        if let Some(i) = target {
            let mut pb = tsp::PathBuilder::new();
            pb.push_path(&out[i].path);
            pb.push_path(&s.path);
            if let Some(p) = pb.finish() {
                out[i].path = p;
                continue;
            }
        }
        out.push(s);
    }
    out
}

fn num(v: f32, decimals: usize) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    let s = match s.as_str() {
        "-0" | "" => "0".to_string(),
        _ => s,
    };
    if let Some(rest) = s.strip_prefix("0.") {
        format!(".{rest}")
    } else if let Some(rest) = s.strip_prefix("-0.") {
        format!("-.{rest}")
    } else {
        s
    }
}

fn push_nums(out: &mut String, vals: &[f32]) {
    for v in vals {
        let n = num(*v, 2);
        let needs_sep = !out.is_empty()
            && !n.starts_with('-')
            && !out.ends_with(|c: char| c.is_ascii_alphabetic())
            && !(n.starts_with('.') && last_number_has_dot(out));
        if needs_sep {
            out.push(' ');
        }
        out.push_str(&n);
    }
}

fn last_number_has_dot(out: &str) -> bool {
    let tail: String = out
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    tail.contains('.')
}

pub fn path_data(path: &tsp::Path) -> String {
    let mut out = String::new();
    let (mut cx, mut cy, mut sx, mut sy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let r = |v: f32| (v * 100.0).round() / 100.0;
    for seg in path.segments() {
        match seg {
            PathSegment::MoveTo(p) => {
                let (x, y) = (r(p.x), r(p.y));
                out.push('m');
                push_nums(&mut out, &[x - cx, y - cy]);
                (cx, cy, sx, sy) = (x, y, x, y);
            }
            PathSegment::LineTo(p) => {
                let (x, y) = (r(p.x), r(p.y));
                if x == cx && y == cy {
                    continue;
                }
                if y == cy {
                    out.push('h');
                    push_nums(&mut out, &[x - cx]);
                } else if x == cx {
                    out.push('v');
                    push_nums(&mut out, &[y - cy]);
                } else {
                    out.push('l');
                    push_nums(&mut out, &[x - cx, y - cy]);
                }
                (cx, cy) = (x, y);
            }
            PathSegment::QuadTo(p1, p) => {
                let (x1, y1, x, y) = (r(p1.x), r(p1.y), r(p.x), r(p.y));
                out.push('q');
                push_nums(&mut out, &[x1 - cx, y1 - cy, x - cx, y - cy]);
                (cx, cy) = (x, y);
            }
            PathSegment::CubicTo(p1, p2, p) => {
                let (x1, y1, x2, y2, x, y) = (r(p1.x), r(p1.y), r(p2.x), r(p2.y), r(p.x), r(p.y));
                out.push('c');
                push_nums(
                    &mut out,
                    &[x1 - cx, y1 - cy, x2 - cx, y2 - cy, x - cx, y - cy],
                );
                (cx, cy) = (x, y);
            }
            PathSegment::Close => {
                out.push('z');
                (cx, cy) = (sx, sy);
            }
        }
    }
    out
}

pub fn parse(svg: &[u8]) -> anyhow::Result<usvg::Tree> {
    let opt = usvg::Options {
        resources_dir: None,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    };
    usvg::Tree::from_data(svg, &opt).context("parsing svg")
}

pub fn render(tree: &usvg::Tree, side: u32) -> anyhow::Result<RgbaImage> {
    let size = tree.size();
    let base = view_transform(size.width(), size.height());
    let s = side as f32 / VIEW as f32;
    let tf = Transform::from_scale(s, s).pre_concat(base);
    let mut pixmap = tiny_skia::Pixmap::new(side, side).context("allocating canvas")?;
    resvg::render(tree, tf, &mut pixmap.as_mut());
    let mut raw = Vec::with_capacity(pixmap.data().len());
    for px in pixmap.pixels() {
        let c = px.demultiply();
        raw.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    RgbaImage::from_raw(side, side, raw).context("building image")
}

pub fn difference(a: &RgbaImage, b: &RgbaImage) -> f32 {
    if a.dimensions() != b.dimensions() {
        return 1.0;
    }
    let mut total = 0.0f64;
    for (pa, pb) in a.pixels().zip(b.pixels()) {
        let (aa, ab) = (f64::from(pa[3]) / 255.0, f64::from(pb[3]) / 255.0);
        for c in 0..3 {
            total += (f64::from(pa[c]) * aa - f64::from(pb[c]) * ab).abs() / 255.0;
        }
        total += (aa - ab).abs();
    }
    (total / (f64::from(a.width()) * f64::from(a.height()) * 4.0)) as f32
}

pub fn verify(svg: &str, max_bytes: usize) -> anyhow::Result<()> {
    ensure!(svg.len() <= max_bytes, "svg exceeds {max_bytes} bytes");
    ensure!(
        svg.starts_with(SVG_PREFIX) && svg.ends_with("</svg>"),
        "unexpected svg envelope"
    );
    let bytes = svg.as_bytes();
    let mut stack: Vec<&str> = Vec::new();
    let mut i = 0;
    let mut roots = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            ensure!(
                bytes[i].is_ascii_whitespace(),
                "text content is not allowed"
            );
            i += 1;
            continue;
        }
        let end = svg[i..]
            .find('>')
            .map(|p| i + p)
            .context("unterminated tag")?;
        let tag = &svg[i + 1..end];
        i = end + 1;
        if let Some(name) = tag.strip_prefix('/') {
            ensure!(stack.pop() == Some(name), "mismatched closing tag");
            continue;
        }
        let self_closing = tag.ends_with('/');
        let tag = tag.trim_end_matches('/');
        let (name, mut rest) = tag.split_once(' ').unwrap_or((tag, ""));
        let parent = stack.last().copied();
        let allowed: &[&str] = match (name, parent) {
            ("svg", None) => &["xmlns", "width", "height", "viewBox"],
            ("defs", Some("svg")) => &[],
            ("path", Some("svg")) => &["fill", "fill-opacity", "fill-rule", "d"],
            ("linearGradient", Some("defs")) => &[
                "id",
                "gradientUnits",
                "x1",
                "y1",
                "x2",
                "y2",
                "gradientTransform",
                "spreadMethod",
            ],
            ("radialGradient", Some("defs")) => &[
                "id",
                "gradientUnits",
                "cx",
                "cy",
                "r",
                "fx",
                "fy",
                "gradientTransform",
                "spreadMethod",
            ],
            ("stop", Some("linearGradient" | "radialGradient")) => {
                &["offset", "stop-color", "stop-opacity"]
            }
            _ => bail!("element <{name}> not allowed here"),
        };
        if name == "svg" {
            roots += 1;
            ensure!(roots == 1, "nested svg");
        }
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                break;
            }
            let (attr, after) = rest.split_once("=\"").context("malformed attribute")?;
            let (value, after) = after.split_once('"').context("unterminated attribute")?;
            ensure!(
                allowed.contains(&attr),
                "attribute {attr} not allowed on <{name}>"
            );
            let ok = match attr {
                "xmlns" => value == "http://www.w3.org/2000/svg",
                "fill" => is_hex_color(value) || is_gradient_ref(value),
                "stop-color" => is_hex_color(value),
                "id" => value
                    .strip_prefix('g')
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
                "fill-rule" => value == "evenodd",
                "gradientUnits" => value == "userSpaceOnUse",
                "spreadMethod" => matches!(value, "reflect" | "repeat"),
                "d" => value
                    .bytes()
                    .all(|b| b"MmLlHhVvCcSsQqTtAaZz0123456789.-+eE ,".contains(&b)),
                "gradientTransform" => value
                    .strip_prefix("matrix(")
                    .and_then(|v| v.strip_suffix(')'))
                    .is_some_and(|v| v.bytes().all(|b| b"0123456789.-+eE ".contains(&b))),
                _ => !value.is_empty() && value.bytes().all(|b| b"0123456789.-+eE ".contains(&b)),
            };
            ensure!(ok, "invalid value for {attr}");
            rest = after;
        }
        if !self_closing {
            stack.push(name);
        } else {
            ensure!(name != "svg", "empty root");
        }
    }
    ensure!(stack.is_empty(), "unclosed elements");
    Ok(())
}

fn is_hex_color(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_gradient_ref(v: &str) -> bool {
    v.strip_prefix("url(#g")
        .and_then(|r| r.strip_suffix(')'))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn hsl(h: f32, s: f32, l: f32) -> String {
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
    let v = [r, g, b].map(|v| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8);
    format!("#{:02x}{:02x}{:02x}", v[0], v[1], v[2])
}

fn tile() -> &'static str {
    "M0 0h128v128H0z"
}

struct SeedRng<'a> {
    seed: &'a [u8; 32],
    i: usize,
}

impl SeedRng<'_> {
    fn next(&mut self) -> f32 {
        let v = self.seed[self.i % self.seed.len()];
        self.i += 1;
        f32::from(v) / 255.0
    }
}

fn readable_lightness(hue: f32, l: f32) -> f32 {
    if (40.0..=190.0).contains(&hue) {
        l.min(0.42)
    } else {
        l
    }
}

fn radial_glow(id: usize, cx: f32, cy: f32, r: f32, colour: &str, opacity: f32) -> String {
    format!(
        r#"<radialGradient id="g{id}" gradientUnits="userSpaceOnUse" cx="{}" cy="{}" r="{}"><stop offset="0" stop-color="{colour}" stop-opacity="{}"/><stop offset="1" stop-color="{colour}" stop-opacity="0"/></radialGradient>"#,
        num(cx, 2),
        num(cy, 2),
        num(r, 2),
        num(opacity, 3)
    )
}

fn layered_background(base: (&str, &str, f32), glows: &[(f32, f32, f32, String, f32)]) -> String {
    let (from, to, angle) = base;
    let (dx, dy) = (angle.cos() * 64.0, angle.sin() * 64.0);
    let mut defs = format!(
        r#"<linearGradient id="g0" gradientUnits="userSpaceOnUse" x1="{}" y1="{}" x2="{}" y2="{}"><stop offset="0" stop-color="{from}"/><stop offset="1" stop-color="{to}"/></linearGradient>"#,
        num(64.0 - dx, 2),
        num(64.0 - dy, 2),
        num(64.0 + dx, 2),
        num(64.0 + dy, 2)
    );
    let mut paths = format!(r#"<path fill="url(#g0)" d="{}"/>"#, tile());
    for (i, (cx, cy, r, colour, opacity)) in glows.iter().enumerate() {
        defs.push_str(&radial_glow(i + 1, *cx, *cy, *r, colour, *opacity));
        let _ = write!(paths, r#"<path fill="url(#g{})" d="{}"/>"#, i + 1, tile());
    }
    format!("<defs>{defs}</defs>{paths}")
}

fn aurora(seed: &[u8; 32]) -> String {
    let mut rng = SeedRng { seed, i: 8 };
    let base = f32::from(u16::from_le_bytes([seed[0], seed[1]]) % 360);
    let offsets: [f32; 3] = match seed[7] % 4 {
        0 => [30.0, 60.0, -30.0],
        1 => [45.0, 90.0, 20.0],
        2 => [-40.0, -80.0, 25.0],
        _ => [60.0, 25.0, 100.0],
    };
    let hues = [
        base,
        base + offsets[0],
        base + offsets[1],
        base + offsets[2],
    ]
    .map(|h| h.rem_euclid(360.0));
    let sat = 0.72 + rng.next() * 0.2;
    let colour = |h: f32, l: f32| hsl(h, sat, readable_lightness(h, l));
    let from = colour(hues[0], 0.36 + rng.next() * 0.08);
    let to = colour(hues[1], 0.46 + rng.next() * 0.08);
    let angle = rng.next() * std::f32::consts::TAU;
    let glows: Vec<(f32, f32, f32, String, f32)> = (0..3)
        .map(|k| {
            let cx = 8.0 + rng.next() * 112.0;
            let cy = 8.0 + rng.next() * 112.0;
            let r = 52.0 + rng.next() * 48.0;
            let hue = hues[1 + k];
            let l = 0.48 + rng.next() * 0.12;
            (cx, cy, r, colour(hue, l), 0.75 + rng.next() * 0.2)
        })
        .collect();
    layered_background((&from, &to, angle), &glows)
}

pub fn identicon(seed: &[u8; 32], mono: bool) -> String {
    let bits = u32::from_le_bytes([seed[3], seed[4], seed[5], seed[6]]);
    let (pad, cell) = (25.6f32, 15.36f32);
    let mut cells = String::new();
    let mut lit = 0;
    for row in 0..5u32 {
        for col in 0..3u32 {
            if bits >> (row * 3 + col) & 1 == 0 {
                continue;
            }
            lit += 1;
            for c in if col == 2 {
                vec![2]
            } else {
                vec![col, 4 - col]
            } {
                let _ = write!(
                    cells,
                    "M{} {}h{}v{}h-{}z",
                    num(pad + c as f32 * cell, 2),
                    num(pad + row as f32 * cell, 2),
                    num(cell, 2),
                    num(cell, 2),
                    num(cell, 2)
                );
            }
        }
    }
    if lit == 0 {
        let _ = write!(
            cells,
            "M{} {}h{}v{}h-{}z",
            num(pad + 2.0 * cell, 2),
            num(pad + 2.0 * cell, 2),
            num(cell, 2),
            num(cell, 2),
            num(cell, 2)
        );
    }
    if mono {
        let svg = format!(r##"{SVG_PREFIX}<path fill="#000000" d="{cells}"/></svg>"##);
        fit(&svg).unwrap_or(svg)
    } else {
        format!(
            r##"{SVG_PREFIX}{}<path fill="#ffffff" fill-opacity=".94" d="{cells}"/></svg>"##,
            aurora(seed)
        )
    }
}

pub fn warning(mono: bool) -> String {
    let mark = "M53.76 30.72A10.24 10.24 0 0 1 74.24 30.72V71.68A10.24 10.24 0 0 1 53.76 71.68z\
                M64 89.6a11.26 11.26 0 1 0 0 22.53a11.26 11.26 0 1 0 0-22.53z";
    if mono {
        let svg = format!(r##"{SVG_PREFIX}<path fill="#000000" d="{mark}"/></svg>"##);
        fit(&svg).unwrap_or(svg)
    } else {
        let background = layered_background(
            ("#b42318", "#e5372a", std::f32::consts::FRAC_PI_4),
            &[
                (22.0, 18.0, 86.0, "#ff7a45".to_string(), 0.75),
                (112.0, 116.0, 80.0, "#7a0f12".to_string(), 0.6),
                (100.0, 20.0, 60.0, "#ff4d6d".to_string(), 0.45),
            ],
        );
        format!(r##"{SVG_PREFIX}{background}<path fill="#ffffff" d="{mark}"/></svg>"##)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(svg: &str) -> Clean {
        clean_tree(&parse(svg.as_bytes()).unwrap(), false, false).unwrap()
    }

    #[test]
    fn strips_everything_but_shapes() {
        let hostile = r##"<?xml version="1.0"?>
        <svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10" onload="alert(1)">
          <script>alert(document.cookie)</script>
          <style>@media (prefers-color-scheme: dark) { rect { fill: url(https://evil.example/x) } } @import url(https://evil.example/a.css);</style>
          <foreignObject width="10" height="10"><iframe xmlns="http://www.w3.org/1999/xhtml" src="https://evil.example"/></foreignObject>
          <image width="10" height="10" href="data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=="/>
          <image width="10" height="10" xlink:href="https://evil.example/track.png"/>
          <a href="javascript:alert(1)"><rect width="10" height="10" fill="#ff0000" onclick="alert(1)"/></a>
          <animate attributeName="fill" to="url(https://evil.example)"/>
          <text x="0" y="5">phish</text>
        </svg>"##;
        let c = clean(hostile);
        verify(&c.svg, 1 << 20).unwrap();
        for bad in [
            "script", "style", "foreign", "image", "href", "on", "evil", "data:", "text",
            "animate", "@",
        ] {
            assert!(!c.svg.contains(bad), "{bad} leaked into {}", c.svg);
        }
        assert!(c.svg.contains("#ff0000"));
        let clipped = clean(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><clipPath id="c"><circle cx="5" cy="5" r="3"/></clipPath><rect width="10" height="10" fill="#00f" clip-path="url(#c)"/></svg>"##,
        );
        assert!(
            clipped.lossy,
            "dropped clip paths must mark the result lossy"
        );
    }

    #[test]
    fn keeps_gradients_and_converts_strokes() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20">
          <defs><linearGradient id="a" x1="0" x2="1"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff" stop-opacity=".5"/></linearGradient></defs>
          <rect width="20" height="20" fill="url(#a)"/>
          <circle cx="10" cy="10" r="5" fill="none" stroke="#00ff00" stroke-width="2"/>
        </svg>"##;
        let c = clean(svg);
        verify(&c.svg, 1 << 20).unwrap();
        assert!(c.svg.contains("<linearGradient"));
        assert!(c.svg.contains("stop-opacity"));
        assert!(c.svg.contains("#00ff00"));
        assert!(!c.svg.contains("stroke"));
        assert!(!c.lossy);
        let a = render(&parse(svg.as_bytes()).unwrap(), 128).unwrap();
        let b = render(&parse(c.svg.as_bytes()).unwrap(), 128).unwrap();
        assert!(difference(&a, &b) < 0.01, "diff {}", difference(&a, &b));
    }

    #[test]
    fn flattens_transforms_and_merges_same_colour() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 32">
          <g transform="translate(4 4) scale(2)"><rect width="4" height="4" fill="#123456"/></g>
          <rect x="40" y="4" width="8" height="8" fill="#123456"/>
          <rect x="20" y="20" width="4" height="4" fill="#abcdef"/>
        </svg>"##;
        let c = clean(svg);
        verify(&c.svg, 1 << 20).unwrap();
        assert_eq!(c.svg.matches("<path").count(), 2);
        assert!(!c.svg.contains("transform=") && !c.svg.contains("<g"));
    }

    #[test]
    fn verifier_rejects_anything_unexpected() {
        let ok = format!(r##"{SVG_PREFIX}<path fill="#000000" d="m0 0h1v1z"/></svg>"##);
        verify(&ok, 1 << 20).unwrap();
        for bad in [
            format!(r##"{SVG_PREFIX}<script>alert(1)</script></svg>"##),
            format!(r##"{SVG_PREFIX}<path fill="url(https://x)" d="m0 0z"/></svg>"##),
            format!(r##"{SVG_PREFIX}<path fill="#000000" d="m0 0z" onload="x"/></svg>"##),
            format!(r##"{SVG_PREFIX}<path fill="#000000" d="m0 0z" style="x"/></svg>"##),
            format!(r##"{SVG_PREFIX}<path fill="#000000" d="m0 0z&#60;"/></svg>"##),
            format!(r##"{SVG_PREFIX}<!-- x --><path fill="#000000" d="m0 0z"/></svg>"##),
            format!(r##"{SVG_PREFIX}text<path fill="#000000" d="m0 0z"/></svg>"##),
            format!(r##"{SVG_PREFIX}<svg></svg></svg>"##),
            format!(r##"{SVG_PREFIX}<stop offset="0" stop-color="#000000"/></svg>"##),
            r##"<svg xmlns="http://www.w3.org/2000/svg" onload="x"><path fill="#000000" d="m0 0z"/></svg>"##.to_string(),
        ] {
            assert!(verify(&bad, 1 << 20).is_err(), "accepted {bad}");
        }
        assert!(verify(&ok, 10).is_err());
    }

    #[test]
    fn generated_icons_are_clean_and_deterministic() {
        let seed = *blake3::hash(b"example.com").as_bytes();
        for mono in [false, true] {
            {
                let a = identicon(&seed, mono);
                assert_eq!(a, identicon(&seed, mono));
                verify(&a, 1 << 16).unwrap();
                verify(&warning(mono), 1 << 16).unwrap();
                let img = render(&parse(a.as_bytes()).unwrap(), 64).unwrap();
                assert!(img.pixels().any(|p| p[3] == 255));
            }
        }
        assert!(!identicon(&seed, true).contains(tile()));
        assert!(!warning(true).contains("Gradient"));
    }

    #[test]
    fn identicon_backgrounds_are_layered_and_seeded() {
        let a = identicon(blake3::hash(b"example.com").as_bytes(), false);
        let b = identicon(blake3::hash(b"example.org").as_bytes(), false);
        assert_ne!(a, b);
        for svg in [&a, &b, &warning(false)] {
            verify(svg, 1 << 16).unwrap();
            assert_eq!(svg.matches("<radialGradient").count(), 3);
            assert_eq!(svg.matches("<linearGradient").count(), 1);
        }
        let backgrounds: std::collections::HashSet<String> = (0u32..64)
            .map(|i| {
                let svg = identicon(blake3::hash(&i.to_le_bytes()).as_bytes(), false);
                svg[..svg.find("</defs>").unwrap()].to_string()
            })
            .collect();
        assert_eq!(backgrounds.len(), 64);
        let img = render(&parse(a.as_bytes()).unwrap(), 128).unwrap();
        assert!(
            img.pixels().all(|p| p[3] == 255),
            "background covers the whole square"
        );
    }

    #[test]
    fn fit_scales_padded_content_to_the_edges() {
        let padded = format!(
            r##"{SVG_PREFIX}<path fill="#ffffff" d="M0 0h128v128H0z"/><path fill="#112233" d="M32 40h64v48H32z"/></svg>"##
        );
        let fitted = fit(&padded).unwrap();
        verify(&fitted, 1 << 16).unwrap();
        assert!(
            !fitted.contains("#ffffff"),
            "white backdrop must be dropped"
        );
        let img = render(&parse(fitted.as_bytes()).unwrap(), 128).unwrap();
        assert_eq!(
            img.get_pixel(1, 64)[3],
            255,
            "content reaches the left edge"
        );
        assert_eq!(
            img.get_pixel(126, 64)[3],
            255,
            "content reaches the right edge"
        );
        assert_eq!(img.get_pixel(64, 8)[3], 0, "aspect ratio is kept");

        let tile = format!(
            r##"{SVG_PREFIX}<path fill="#1db954" d="M0 0h128v128H0z"/><path fill="#ffffff" d="M40 40h48v48H40z"/></svg>"##
        );
        let kept = fit(&tile).unwrap();
        assert!(
            kept.contains("#1db954"),
            "coloured tiles are part of the design"
        );

        let gradient = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><defs><linearGradient id="a" x1="25" x2="75" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><rect x="25" y="25" width="50" height="50" fill="url(#a)"/></svg>"##;
        let c = clean_tree(&parse(gradient.as_bytes()).unwrap(), false, false).unwrap();
        let g = fit(&c.svg).unwrap();
        let img = render(&parse(g.as_bytes()).unwrap(), 128).unwrap();
        let (left, right) = (img.get_pixel(2, 64), img.get_pixel(125, 64));
        assert!(
            left[0] > 200 && left[2] < 60,
            "gradient follows the scaled shape: {left:?}"
        );
        assert!(
            right[2] > 200 && right[0] < 60,
            "gradient follows the scaled shape: {right:?}"
        );
    }

    #[test]
    fn formats_compact_numbers() {
        assert_eq!(num(0.5, 2), ".5");
        assert_eq!(num(-0.25, 2), "-.25");
        assert_eq!(num(3.0, 2), "3");
        assert_eq!(num(-0.0001, 2), "0");
        let mut s = String::from("m");
        push_nums(&mut s, &[0.5, 0.25, -1.0, 2.0]);
        assert_eq!(s, "m.5.25-1 2");
    }
}
