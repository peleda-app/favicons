use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::config::{IndexFormat, SlugCatalog};
use crate::net::{HttpClient, read_source};

pub const MAX_SLUG: usize = 64;
const MAX_TAGS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(skip)]
    color: String,
    #[serde(skip)]
    mono: Option<String>,
    #[serde(skip)]
    tint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    license: Option<String>,
    #[serde(skip)]
    inline: Option<Packed>,
    #[serde(skip)]
    mono_inline: Option<Packed>,
    #[serde(skip)]
    source: Option<String>,
    #[serde(skip)]
    dropped: bool,
}

impl Item {
    fn source_key(&self) -> Option<String> {
        if self.inline.is_some() || self.dropped {
            return self.source.clone();
        }
        let raw = format!("{}|{:?}|{:?}", self.color, self.mono, self.tint);
        Some(blake3::hash(raw.as_bytes()).to_hex()[..20].to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Packed(Arc<[u8]>);

impl Packed {
    fn new(svg: &str) -> Self {
        Self(miniz_oxide::deflate::compress_to_vec(svg.as_bytes(), 6).into())
    }

    fn hash(&self, mono: Option<&Packed>) -> String {
        let mut h = blake3::Hasher::new();
        h.update(&self.0);
        if let Some(m) = mono {
            h.update(&m.0);
        }
        h.finalize().to_hex()[..20].to_string()
    }

    fn unpack(&self) -> Option<Vec<u8>> {
        miniz_oxide::inflate::decompress_to_vec_with_limit(&self.0, 4 << 20).ok()
    }
}

pub struct Entry {
    pub color: Url,
    pub mono: Option<Url>,
    pub tint: Option<String>,
    pub license: String,
    pub inline: Option<Vec<u8>>,
    pub mono_inline: Option<Vec<u8>>,
    pub source: Option<String>,
    pub dropped: bool,
}

struct Index {
    items: HashMap<String, Item>,
    listing: Bytes,
}

pub struct Catalog {
    cfg: SlugCatalog,
    base: Url,
    index: RwLock<Arc<Index>>,
    pub wake: tokio::sync::Notify,
}

#[derive(Serialize)]
struct Listing<'a> {
    id: &'a str,
    name: &'a str,
    license: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    attribution: Option<&'a str>,
    count: usize,
    items: &'a [&'a Item],
}

#[derive(Serialize)]
pub struct Summary<'a> {
    id: &'a str,
    name: &'a str,
    license: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    attribution: Option<&'a str>,
    count: usize,
}

pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_SLUG
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn is_asset_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 200
        && !p.starts_with('/')
        && !p.contains("..")
        && (p.ends_with(".svg") || p.ends_with(".png"))
        && p.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
}

fn is_hex(s: &str) -> bool {
    s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn key(id: &str, slug: &str) -> String {
    format!("@{id}.{slug}")
}

pub fn parse_key(key: &str) -> Option<(&str, &str)> {
    key.strip_prefix('@')?.split_once('.')
}

pub fn tint_svg(svg: &[u8], hex: &str) -> Option<Vec<u8>> {
    if !is_hex(hex) {
        return None;
    }
    let text = std::str::from_utf8(svg).ok()?;
    let at = text.find("<svg")? + "<svg".len();
    let mut out = String::with_capacity(text.len() + 16);
    out.push_str(&text[..at]);
    out.push_str(&format!(" fill=\"#{}\"", hex.to_ascii_lowercase()));
    out.push_str(&text[at..]);
    Some(out.into_bytes())
}

impl Catalog {
    pub fn new(cfg: SlugCatalog) -> Option<Self> {
        let base = Url::parse(&cfg.base_url).ok()?;
        let c = Self {
            cfg,
            base,
            index: RwLock::new(Arc::new(Index {
                items: HashMap::new(),
                listing: Bytes::new(),
            })),
            wake: tokio::sync::Notify::new(),
        };
        let listing = c.render_listing(&HashMap::new());
        if let Ok(mut g) = c.index.write() {
            *g = Arc::new(Index {
                items: HashMap::new(),
                listing,
            });
        }
        Some(c)
    }

    pub fn id(&self) -> &str {
        &self.cfg.id
    }

    pub fn reload_secs(&self) -> u64 {
        self.cfg.reload_secs
    }

    pub fn prerender(&self) -> bool {
        self.cfg.prerender
    }

    pub fn sources(&self, remote: bool) -> Vec<(String, String)> {
        self.current()
            .items
            .values()
            .filter(|i| i.inline.is_some() || (remote && !i.dropped))
            .filter_map(|i| Some((i.slug.clone(), i.source_key()?)))
            .collect()
    }

    pub fn drop_sources(&self, done: &std::collections::HashSet<String>) {
        let index = self.current();
        let mut items = index.items.clone();
        for slug in done {
            if let Some(item) = items.get_mut(slug)
                && item.inline.is_some()
            {
                item.inline = None;
                item.mono_inline = None;
                item.dropped = true;
            }
        }
        if let Ok(mut g) = self.index.write() {
            *g = Arc::new(Index {
                items,
                listing: index.listing.clone(),
            });
        }
    }

    fn current(&self) -> Arc<Index> {
        self.index
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|p| p.into_inner().clone())
    }

    pub fn is_loaded(&self) -> bool {
        !self.current().items.is_empty()
    }

    pub fn contains(&self, slug: &str) -> bool {
        self.current().items.contains_key(slug)
    }

    pub fn listing(&self) -> Bytes {
        self.current().listing.clone()
    }

    pub fn summary(&self) -> Summary<'_> {
        Summary {
            id: &self.cfg.id,
            name: &self.cfg.name,
            license: &self.cfg.license,
            attribution: self.cfg.attribution.as_deref(),
            count: self.current().items.len(),
        }
    }

    pub fn lookup(&self, slug: &str) -> Option<Entry> {
        let index = self.current();
        let item = index.items.get(slug)?;
        let join = |p: &str| {
            let url = self.base.join(p).ok()?;
            (url.scheme() == "https" && url.as_str().starts_with(self.base.as_str())).then_some(url)
        };
        Some(Entry {
            color: join(&item.color)?,
            mono: item.mono.as_deref().and_then(join),
            tint: item.tint.clone(),
            license: item
                .license
                .clone()
                .unwrap_or_else(|| self.cfg.license.clone()),
            inline: item.inline.as_ref().and_then(Packed::unpack),
            mono_inline: item.mono_inline.as_ref().and_then(Packed::unpack),
            source: item.source_key(),
            dropped: item.dropped,
        })
    }

    pub async fn reload(&self, client: &HttpClient) {
        let id = self.cfg.id.as_str();
        let mut items = match read_source(client, &self.cfg.index_url, self.cfg.max_index_bytes)
            .await
        {
            Ok(bytes) => match parse_index(self.cfg.format, &bytes, &self.cfg.licenses) {
                Ok(items) if !items.is_empty() => items,
                Ok(_) => {
                    tracing::warn!(catalog = id, "catalog index is empty; keeping previous one");
                    return;
                }
                Err(e) => {
                    tracing::warn!(catalog = id, error = %e, "catalog index rejected; keeping previous one");
                    return;
                }
            },
            Err(e) => {
                tracing::warn!(catalog = id, error = %e, "catalog index download failed; keeping previous one");
                return;
            }
        };
        if let Some(url) = &self.cfg.mono_index_url {
            match read_source(client, url, self.cfg.max_index_bytes).await {
                Ok(bytes) => {
                    if let Err(e) = merge_iconify_mono(&mut items, &bytes) {
                        tracing::warn!(catalog = id, error = %e, "mono index rejected");
                    }
                }
                Err(e) => tracing::warn!(catalog = id, error = %e, "mono index download failed"),
            }
        }
        let listing = self.render_listing(&items);
        tracing::info!(catalog = id, entries = items.len(), "catalog loaded");
        if let Ok(mut g) = self.index.write() {
            *g = Arc::new(Index { items, listing });
        }
    }

    #[cfg(test)]
    pub fn load(&self, json: &[u8]) {
        let items = parse_index(self.cfg.format, json, &self.cfg.licenses).unwrap();
        let listing = self.render_listing(&items);
        *self.index.write().unwrap() = Arc::new(Index { items, listing });
    }

    fn render_listing(&self, items: &HashMap<String, Item>) -> Bytes {
        let mut sorted: Vec<&Item> = items.values().collect();
        sorted.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.slug.cmp(&b.slug))
        });
        let listing = Listing {
            id: &self.cfg.id,
            name: &self.cfg.name,
            license: &self.cfg.license,
            attribution: self.cfg.attribution.as_deref(),
            count: sorted.len(),
            items: &sorted,
        };
        Bytes::from(serde_json::to_vec(&listing).unwrap_or_default())
    }
}

pub struct Catalogs(Vec<Arc<Catalog>>);

impl Catalogs {
    pub fn new(cfgs: &[SlugCatalog]) -> Self {
        Self(
            cfgs.iter()
                .cloned()
                .filter_map(Catalog::new)
                .map(Arc::new)
                .collect(),
        )
    }

    pub fn get(&self, id: &str) -> Option<&Arc<Catalog>> {
        self.0.iter().find(|c| c.id() == id)
    }

    pub fn all(&self) -> &[Arc<Catalog>] {
        &self.0
    }

    pub fn summaries(&self) -> Bytes {
        let list: Vec<Summary<'_>> = self.0.iter().map(|c| c.summary()).collect();
        Bytes::from(serde_json::to_vec(&list).unwrap_or_default())
    }
}

fn clean(s: &str, max: usize) -> String {
    s.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect()
}

fn tags<'a>(it: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    it.into_iter()
        .map(|t| clean(t, 40))
        .filter(|t| !t.is_empty())
        .take(MAX_TAGS)
        .collect()
}

fn title_case(slug: &str) -> String {
    slug.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Deserialize)]
struct IconifySet {
    #[serde(default)]
    icons: HashMap<String, IconifyIcon>,
    #[serde(default)]
    aliases: HashMap<String, IconifyAlias>,
    #[serde(default)]
    categories: HashMap<String, Vec<String>>,
    #[serde(default)]
    left: f64,
    #[serde(default)]
    top: f64,
    #[serde(default = "iconify_side")]
    width: f64,
    #[serde(default = "iconify_side")]
    height: f64,
}

#[derive(Deserialize)]
struct IconifyIcon {
    body: String,
    left: Option<f64>,
    top: Option<f64>,
    width: Option<f64>,
    height: Option<f64>,
    #[serde(default)]
    hidden: bool,
}

#[derive(Deserialize)]
struct IconifyAlias {
    parent: String,
}

fn iconify_side() -> f64 {
    16.0
}

impl IconifySet {
    fn svg(&self, icon: &IconifyIcon) -> String {
        let (l, t) = (icon.left.unwrap_or(self.left), icon.top.unwrap_or(self.top));
        let (w, h) = (
            icon.width.unwrap_or(self.width),
            icon.height.unwrap_or(self.height),
        );
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{l} {t} {w} {h}\" width=\"{w}\" height=\"{h}\">{}</svg>",
            icon.body.replace("currentColor", "#000")
        )
    }
}

pub fn merge_iconify_mono(items: &mut HashMap<String, Item>, json: &[u8]) -> anyhow::Result<()> {
    let set: IconifySet = serde_json::from_slice(json)?;
    for (slug, icon) in &set.icons {
        if let Some(item) = items.get_mut(slug) {
            let mono = Packed::new(&set.svg(icon));
            item.source = item.inline.as_ref().map(|c| c.hash(Some(&mono)));
            item.mono_inline = Some(mono);
        }
    }
    Ok(())
}

fn insert(items: &mut HashMap<String, Item>, item: Item) {
    let ok = is_slug(&item.slug)
        && is_asset_path(&item.color)
        && item.mono.as_deref().is_none_or(is_asset_path)
        && item.tint.as_deref().is_none_or(is_hex);
    if ok {
        items.insert(item.slug.clone(), item);
    }
}

fn yes(v: &str) -> bool {
    v.eq_ignore_ascii_case("yes")
}

#[derive(Deserialize)]
struct SelfhstEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Reference")]
    reference: String,
    #[serde(rename = "SVG", default)]
    svg: String,
    #[serde(rename = "PNG", default)]
    png: String,
    #[serde(rename = "Dark", default)]
    dark: String,
    #[serde(rename = "Category", default)]
    category: String,
    #[serde(rename = "Tags", default)]
    tags: String,
}

#[derive(Deserialize)]
struct SimpleIconsEntry {
    title: String,
    #[serde(default)]
    slug: String,
    hex: String,
    #[serde(default)]
    aliases: SimpleIconsAliases,
}

#[derive(Deserialize, Default)]
struct SimpleIconsAliases {
    #[serde(default)]
    aka: Vec<String>,
}

#[derive(Deserialize)]
struct DashboardEntry {
    #[serde(default)]
    base: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
}

#[derive(Deserialize)]
struct DeviconEntry {
    name: String,
    #[serde(default)]
    altnames: Vec<String>,
    #[serde(default)]
    tags: Vec<String>,
    versions: DeviconVersions,
}

#[derive(Deserialize)]
struct DeviconVersions {
    #[serde(default)]
    svg: Vec<String>,
}

#[derive(Deserialize)]
struct ThesvgEntry {
    slug: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    license: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    categories: Vec<String>,
    #[serde(default)]
    variants: HashMap<String, String>,
}

pub fn parse_index(
    format: IndexFormat,
    json: &[u8],
    licenses: &[String],
) -> anyhow::Result<HashMap<String, Item>> {
    let mut items = HashMap::new();
    match format {
        IndexFormat::Selfhst => {
            for e in serde_json::from_slice::<Vec<SelfhstEntry>>(json)? {
                let slug = e.reference.trim().to_ascii_lowercase();
                let svg = yes(&e.svg);
                let color = if svg {
                    format!("svg/{slug}.svg")
                } else if yes(&e.png) {
                    format!("png/{slug}.png")
                } else {
                    continue;
                };
                insert(
                    &mut items,
                    Item {
                        name: clean(&e.name, 80),
                        category: clean(&e.category, 40),
                        tags: tags(e.tags.split(',')),
                        mono: (svg && yes(&e.dark)).then(|| format!("svg/{slug}-dark.svg")),
                        color,
                        tint: None,
                        license: None,
                        inline: None,
                        mono_inline: None,
                        source: None,
                        dropped: false,
                        slug,
                    },
                );
            }
        }
        IndexFormat::SimpleIcons => {
            for e in serde_json::from_slice::<Vec<SimpleIconsEntry>>(json)? {
                let slug = e.slug.trim().to_ascii_lowercase();
                let path = format!("icons/{slug}.svg");
                insert(
                    &mut items,
                    Item {
                        name: clean(&e.title, 80),
                        category: String::new(),
                        tags: tags(e.aliases.aka.iter().map(String::as_str)),
                        color: path.clone(),
                        mono: Some(path),
                        tint: Some(e.hex.to_ascii_lowercase()),
                        license: None,
                        inline: None,
                        mono_inline: None,
                        source: None,
                        dropped: false,
                        slug,
                    },
                );
            }
        }
        IndexFormat::DashboardIcons => {
            let map: HashMap<String, DashboardEntry> = serde_json::from_slice(json)?;
            for (slug, e) in map {
                let ext = match e.base.as_str() {
                    "svg" => "svg",
                    "png" => "png",
                    _ => continue,
                };
                insert(
                    &mut items,
                    Item {
                        name: title_case(&slug),
                        category: e
                            .categories
                            .first()
                            .map(|c| clean(c, 40))
                            .unwrap_or_default(),
                        tags: tags(e.aliases.iter().map(String::as_str)),
                        color: format!("{ext}/{slug}.{ext}"),
                        mono: None,
                        tint: None,
                        license: None,
                        inline: None,
                        mono_inline: None,
                        source: None,
                        dropped: false,
                        slug,
                    },
                );
            }
        }
        IndexFormat::Devicon => {
            for e in serde_json::from_slice::<Vec<DeviconEntry>>(json)? {
                let Some(variant) = [
                    "original",
                    "plain",
                    "original-wordmark",
                    "plain-wordmark",
                    "line",
                ]
                .into_iter()
                .find(|v| e.versions.svg.iter().any(|s| s == v)) else {
                    continue;
                };
                let slug = e.name.trim().to_ascii_lowercase();
                insert(
                    &mut items,
                    Item {
                        name: title_case(&slug),
                        category: String::new(),
                        tags: tags(e.altnames.iter().chain(&e.tags).map(String::as_str)),
                        color: format!("icons/{slug}/{slug}-{variant}.svg"),
                        mono: None,
                        tint: None,
                        license: None,
                        inline: None,
                        mono_inline: None,
                        source: None,
                        dropped: false,
                        slug,
                    },
                );
            }
        }
        IndexFormat::Iconify => {
            let set: IconifySet = serde_json::from_slice(json)?;
            let mut tags_of: HashMap<&str, Vec<&str>> = HashMap::new();
            for (alias, a) in &set.aliases {
                tags_of
                    .entry(a.parent.as_str())
                    .or_default()
                    .push(alias.as_str());
            }
            let mut category_of: HashMap<&str, &str> = HashMap::new();
            for (category, names) in &set.categories {
                for n in names {
                    category_of.entry(n.as_str()).or_insert(category.as_str());
                }
            }
            for (slug, icon) in &set.icons {
                if icon.hidden {
                    continue;
                }
                insert(
                    &mut items,
                    Item {
                        name: title_case(slug),
                        category: category_of
                            .get(slug.as_str())
                            .map(|c| clean(c, 40))
                            .unwrap_or_default(),
                        tags: tags(
                            tags_of
                                .get(slug.as_str())
                                .into_iter()
                                .flatten()
                                .map(|t| t.replace('-', " "))
                                .collect::<Vec<_>>()
                                .iter()
                                .map(String::as_str),
                        ),
                        color: format!("{slug}.svg"),
                        mono: None,
                        tint: None,
                        license: None,
                        inline: Some(Packed::new(&set.svg(icon))),
                        mono_inline: None,
                        source: None,
                        dropped: false,
                        slug: slug.clone(),
                    },
                );
            }
        }
        IndexFormat::Thesvg => {
            for e in serde_json::from_slice::<Vec<ThesvgEntry>>(json)? {
                if !licenses.is_empty() && !licenses.contains(&e.license) {
                    continue;
                }
                let Some(color) = e
                    .variants
                    .get("default")
                    .map(|p| p.trim_start_matches('/').to_string())
                else {
                    continue;
                };
                insert(
                    &mut items,
                    Item {
                        name: clean(
                            if e.title.is_empty() {
                                &e.slug
                            } else {
                                &e.title
                            },
                            80,
                        ),
                        category: e
                            .categories
                            .first()
                            .map(|c| clean(c, 40))
                            .unwrap_or_default(),
                        tags: tags(e.aliases.iter().map(String::as_str)),
                        mono: e
                            .variants
                            .get("mono")
                            .map(|p| p.trim_start_matches('/').to_string()),
                        color,
                        tint: None,
                        license: Some(clean(&e.license, 40)),
                        inline: None,
                        mono_inline: None,
                        source: None,
                        dropped: false,
                        slug: e.slug.trim().to_ascii_lowercase(),
                    },
                );
            }
        }
    }
    for item in items.values_mut() {
        if item.source.is_none() {
            item.source = item.inline.as_ref().map(|c| c.hash(None));
        }
    }
    Ok(items)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn cfg() -> SlugCatalog {
        SlugCatalog {
            id: "selfhst".into(),
            name: "selfh.st icons".into(),
            format: IndexFormat::Selfhst,
            index_url: "https://cdn.example/index.json".into(),
            mono_index_url: None,
            base_url: "https://cdn.example/selfhst/".into(),
            license: "CC-BY-4.0".into(),
            licenses: vec![],
            attribution: Some("https://selfh.st/icons/".into()),
            reload_secs: 86_400,
            max_index_bytes: 1 << 20,
            prerender: true,
        }
    }

    pub const INDEX: &str = r#"[
      {"Name":"Jellyfin","Reference":"jellyfin","SVG":"Yes","PNG":"Yes","Dark":"Yes","Category":"Self-Hosted","Tags":"media, streaming"},
      {"Name":"Raster Only","Reference":"raster-only","SVG":"No","PNG":"Yes","Dark":"No","Category":"Self-Hosted","Tags":""},
      {"Name":"Nothing","Reference":"nothing","SVG":"No","PNG":"No"},
      {"Name":"Bad","Reference":"../etc/passwd","SVG":"Yes"},
      {"Name":"Upper","Reference":"Home-Assistant","SVG":"Yes","Dark":"No"}
    ]"#;

    fn catalog(format: IndexFormat, json: &str) -> Catalog {
        let licenses = vec!["CC0-1.0".into(), "MIT".into()];
        let c = Catalog::new(SlugCatalog {
            format,
            licenses,
            ..cfg()
        })
        .unwrap();
        c.load(json.as_bytes());
        c
    }

    #[test]
    fn builds_iconify_svgs_with_aliases_categories_and_masks() {
        let colour = r##"{"prefix":"fluent-emoji-flat","width":32,"height":32,
            "icons":{"grinning-face":{"body":"<path fill=\"#FFB02E\" d=\"M0 0h32v32H0z\"/>"},
                     "wide-thing":{"body":"<path d=\"M0 0h1v1H0z\"/>","width":48,"left":-8},
                     "secret":{"body":"<path/>","hidden":true},
                     "Bad Slug":{"body":"<path/>"}},
            "aliases":{"grinning":{"parent":"grinning-face"}},
            "categories":{"Smileys":["grinning-face"]}}"##;
        let c = catalog(IndexFormat::Iconify, colour);
        assert!(c.contains("grinning-face") && c.contains("wide-thing"));
        assert!(!c.contains("secret") && !c.contains("bad slug"));
        let e = c.lookup("grinning-face").unwrap();
        let svg = String::from_utf8(e.inline.unwrap()).unwrap();
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\""));
        assert!(svg.contains("#FFB02E"));
        let wide = String::from_utf8(c.lookup("wide-thing").unwrap().inline.unwrap()).unwrap();
        assert!(wide.contains("viewBox=\"-8 0 48 32\""));
        let listing: serde_json::Value = serde_json::from_slice(&c.listing()).unwrap();
        let item = &listing["items"][0];
        assert_eq!(item["name"], "Grinning Face");
        assert_eq!(item["category"], "Smileys");
        assert_eq!(item["tags"][0], "grinning");

        let mut items = parse_index(IndexFormat::Iconify, colour.as_bytes(), &[]).unwrap();
        let mono = r#"{"width":32,"height":32,"icons":{"grinning-face":{"body":"<path fill=\"currentColor\" d=\"M1 1h30v30H1z\"/>"}}}"#;
        merge_iconify_mono(&mut items, mono.as_bytes()).unwrap();
        let m = items["grinning-face"]
            .mono_inline
            .as_ref()
            .unwrap()
            .unpack()
            .unwrap();
        let m = String::from_utf8(m).unwrap();
        assert!(m.contains("fill=\"#000\"") && !m.contains("currentColor"));
        assert!(items["wide-thing"].mono_inline.is_none());
    }

    #[test]
    fn parses_selfhst_and_drops_unsafe_slugs() {
        let items = parse_index(IndexFormat::Selfhst, INDEX.as_bytes(), &[]).unwrap();
        let mut slugs: Vec<_> = items.keys().cloned().collect();
        slugs.sort();
        assert_eq!(slugs, ["home-assistant", "jellyfin", "raster-only"]);
        assert_eq!(items["jellyfin"].tags, ["media", "streaming"]);
        let c = catalog(IndexFormat::Selfhst, INDEX);
        let e = c.lookup("jellyfin").unwrap();
        assert_eq!(
            e.color.as_str(),
            "https://cdn.example/selfhst/svg/jellyfin.svg"
        );
        assert_eq!(
            e.mono.unwrap().as_str(),
            "https://cdn.example/selfhst/svg/jellyfin-dark.svg"
        );
        assert_eq!(
            c.lookup("raster-only").unwrap().color.as_str(),
            "https://cdn.example/selfhst/png/raster-only.png"
        );
        assert!(c.lookup("missing").is_none());
        let json: serde_json::Value = serde_json::from_slice(&c.listing()).unwrap();
        assert_eq!(json["count"], 3);
        assert_eq!(json["items"][0]["slug"], "jellyfin");
        assert!(json["items"][0].get("color").is_none());
    }

    #[test]
    fn parses_simple_icons_with_brand_tints() {
        let c = catalog(
            IndexFormat::SimpleIcons,
            r#"[{"title":"GitHub","slug":"github","hex":"181717","aliases":{"aka":["GH"]}},
                {"title":"Bad Hex","slug":"bad","hex":"zzz"},
                {"title":"No Slug","hex":"ffffff"}]"#,
        );
        let e = c.lookup("github").unwrap();
        assert_eq!(
            e.color.as_str(),
            "https://cdn.example/selfhst/icons/github.svg"
        );
        assert_eq!(
            e.mono.unwrap().as_str(),
            "https://cdn.example/selfhst/icons/github.svg"
        );
        assert_eq!(e.tint.as_deref(), Some("181717"));
        assert!(c.lookup("bad").is_none());
        let tinted = tint_svg(
            br#"<svg role="img" viewBox="0 0 24 24"><path d="M0 0"/></svg>"#,
            "181717",
        )
        .unwrap();
        assert!(
            String::from_utf8(tinted)
                .unwrap()
                .starts_with(r##"<svg fill="#181717" role="img""##)
        );
        assert!(tint_svg(b"<svg>", "red\"").is_none());
    }

    #[test]
    fn parses_dashboard_icons_devicon_and_thesvg() {
        let dash = catalog(
            IndexFormat::DashboardIcons,
            r#"{"home-assistant":{"base":"svg","aliases":["HA"],"categories":["Home"]},
                "old-app":{"base":"png"},"webp-only":{"base":"webp"}}"#,
        );
        assert_eq!(
            dash.lookup("home-assistant").unwrap().color.path(),
            "/selfhst/svg/home-assistant.svg"
        );
        assert_eq!(
            dash.lookup("old-app").unwrap().color.path(),
            "/selfhst/png/old-app.png"
        );
        assert!(dash.lookup("webp-only").is_none());
        let json: serde_json::Value = serde_json::from_slice(&dash.listing()).unwrap();
        assert_eq!(json["items"][0]["name"], "Home Assistant");

        let dev = catalog(
            IndexFormat::Devicon,
            r#"[{"name":"rust","altnames":[],"tags":["language"],"versions":{"svg":["plain","original"]}},
                {"name":"onlyline","versions":{"svg":["line"]}},
                {"name":"nothing","versions":{"svg":[]}}]"#,
        );
        assert_eq!(
            dev.lookup("rust").unwrap().color.path(),
            "/selfhst/icons/rust/rust-original.svg"
        );
        assert_eq!(
            dev.lookup("onlyline").unwrap().color.path(),
            "/selfhst/icons/onlyline/onlyline-line.svg"
        );
        assert!(dev.lookup("nothing").is_none());

        let svg = catalog(
            IndexFormat::Thesvg,
            r#"[{"slug":"stripe","title":"Stripe","license":"CC0-1.0","collection":"brands","variants":{"default":"/icons/stripe/default.svg","mono":"/icons/stripe/mono.svg"}},
                {"slug":"mit","license":"MIT","collection":"azure","variants":{"default":"/icons/mit/default.svg"}},
                {"slug":"nd","license":"CC-BY-ND-2.0","collection":"aws","variants":{"default":"/icons/nd/default.svg"}},
                {"slug":"evil","license":"CC0-1.0","collection":"brands","variants":{"default":"/../../etc/passwd.svg"}}]"#,
        );
        let e = svg.lookup("stripe").unwrap();
        assert_eq!(e.color.path(), "/selfhst/icons/stripe/default.svg");
        assert_eq!(e.mono.unwrap().path(), "/selfhst/icons/stripe/mono.svg");
        assert_eq!(e.license, "CC0-1.0");
        assert_eq!(svg.lookup("mit").unwrap().license, "MIT");
        assert!(svg.lookup("nd").is_none());
        assert!(svg.lookup("evil").is_none());
    }

    #[test]
    fn keys_round_trip_and_never_look_like_domains() {
        let k = key("selfhst", "jellyfin");
        assert_eq!(parse_key(&k), Some(("selfhst", "jellyfin")));
        assert!(parse_key("jellyfin.org").is_none());
        assert!(!is_slug("Jelly"));
        assert!(!is_slug("a/b"));
        assert!(!is_slug(""));
    }
}
