use serde_json::{Value, json};

use crate::contract::{FallbackReason, MonoSource, Provenance, Source, Style};
use crate::palette::{Fill, Shape};
use crate::plate::Plate;
use crate::vector::VectorOrigin;

pub struct Spec<'a> {
    pub origin: &'a str,
    pub catalogs: &'a [String],
    pub site: bool,
}

fn names<T: Copy>(all: &[T], name: impl Fn(T) -> &'static str) -> Value {
    Value::from(all.iter().map(|v| name(*v)).collect::<Vec<_>>())
}

fn enum_param(name: &str, description: &str, values: Value) -> Value {
    json!({
        "name": name,
        "in": "path",
        "required": true,
        "description": description,
        "schema": { "type": "string", "enum": values }
    })
}

fn text(description: &str, media: &str) -> Value {
    json!({ "description": description, "content": { media: { "schema": { "type": "string" } } } })
}

impl Spec<'_> {
    pub fn render(&self) -> String {
        let host = self
            .origin
            .split_once("://")
            .map_or(self.origin, |(_, h)| h);
        let style = enum_param(
            "style",
            "color: the icon in its own colours. mono: one black shape for CSS masks.",
            names(&Style::ALL, Style::as_str),
        );
        let domain = json!({
            "name": "domain",
            "in": "path",
            "required": true,
            "description": "Registrable domain, e.g. github.com. Any other form gets a 301 to it.",
            "schema": { "type": "string", "examples": ["discord.com"] }
        });
        let shape = enum_param(
            "shape",
            "rounded: 22.5% corner radius. superellipse: n = 5, like iOS app icons.",
            Value::from(Plate::SHAPES.to_vec()),
        );
        let theme = enum_param(
            "theme",
            "Which theme's plate and mask colours to use.",
            Value::from(Plate::THEMES.to_vec()),
        );
        let icon = json!({ "$ref": "#/components/responses/Icon" });
        let mut paths = serde_json::Map::new();
        paths.insert("/{style}/{domain}".into(), json!({ "get": {
            "operationId": "getIcon",
            "summary": "Icon of a website, as an edge-to-edge SVG",
            "parameters": [style, domain, { "$ref": "#/components/parameters/IfNoneMatch" }],
            "responses": {
                "200": icon,
                "301": { "description": "Non-canonical domain or path; Location holds the canonical URL." },
                "304": { "description": "If-None-Match matched." },
                "404": { "description": "Unknown style or path." },
                "429": { "description": "Rate limit for this network; retry after Retry-After seconds." },
                "503": { "description": "Overloaded or timed out; retry shortly." }
            }
        }}));
        paths.insert(
            "/{style}/{domain}/{shape}/{theme}".into(),
            json!({ "get": {
                "operationId": "getPlatedIcon",
                "summary": "Icon drawn on its themed plate",
                "parameters": [style, domain, shape, theme],
                "responses": {
                    "200": icon,
                    "301": { "description": "Non-canonical domain." },
                    "404": { "description": "Unknown style, shape or theme." }
                }
            }}),
        );
        paths.insert(
            "/{style}/{domain}/{shape}".into(),
            json!({ "get": {
                "operationId": "getAutoPlatedIcon",
                "summary": "Icon on its plate, following the system light or dark scheme",
                "description": "Both themes' plate and mask colours are in the SVG; a prefers-color-scheme media query picks one.",
                "parameters": [style, domain, shape],
                "responses": {
                    "200": icon,
                    "301": { "description": "Non-canonical domain." },
                    "404": { "description": "Unknown style or shape." }
                }
            }}),
        );
        if !self.catalogs.is_empty() {
            let catalog = enum_param(
                "catalog",
                "Catalog id; /catalogs.json lists them.",
                Value::from(self.catalogs.to_vec()),
            );
            let slug = json!({
                "name": "slug",
                "in": "path",
                "required": true,
                "description": "Icon slug; /catalogs/{id}.json lists them.",
                "schema": { "type": "string", "pattern": "^[a-z0-9-]{1,64}$", "examples": ["jellyfin"] }
            });
            let responses = json!({
                "200": icon,
                "301": { "description": "Non-canonical slug." },
                "404": { "description": "Unknown style, catalog or slug." },
                "503": { "description": "The catalog index is still loading." }
            });
            paths.insert(
                "/{style}/{catalog}/{slug}".into(),
                json!({ "get": {
                    "operationId": "getCatalogIcon",
                    "summary": "Icon from an optional catalog, by slug",
                    "parameters": [style, catalog, slug],
                    "responses": responses
                }}),
            );
            paths.insert(
                "/{style}/{catalog}/{slug}/{shape}/{theme}".into(),
                json!({ "get": {
                    "operationId": "getPlatedCatalogIcon",
                    "summary": "Catalog icon drawn on its themed plate",
                    "parameters": [style, catalog, slug, shape, theme],
                    "responses": responses.clone()
                }}),
            );
            paths.insert(
                "/{style}/{catalog}/{slug}/{shape}".into(),
                json!({ "get": {
                    "operationId": "getAutoPlatedCatalogIcon",
                    "summary": "Catalog icon on its plate, following the system light or dark scheme",
                    "parameters": [style, catalog, slug, shape],
                    "responses": responses
                }}),
            );
        }
        paths.insert(
            "/catalogs.json".into(),
            json!({ "get": {
                "operationId": "listCatalogs",
                "summary": "Configured catalogs: id, name, license, attribution, count",
                "responses": { "200": text("JSON array", "application/json") }
            }}),
        );
        paths.insert("/catalogs/{file}".into(), json!({ "get": {
            "operationId": "catalogItems",
            "summary": "Every slug in a catalog with its name, category and tags",
            "parameters": [{ "name": "file", "in": "path", "required": true, "description": "{id}.json", "schema": { "type": "string" } }],
            "responses": { "200": text("JSON object", "application/json"), "404": { "description": "Unknown catalog." } }
        }}));
        let root = if self.site {
            json!({ "operationId": "page", "summary": "The demo page", "responses": { "200": text("HTML", "text/html") } })
        } else {
            json!({ "operationId": "root", "summary": "This description", "responses": { "200": text("OpenAPI 3.1", "application/json") } })
        };
        paths.insert("/".into(), json!({ "get": root }));
        paths.insert(
            "/openapi.json".into(),
            json!({ "get": {
                "operationId": "openapi", "summary": "This description",
                "responses": { "200": text("OpenAPI 3.1", "application/json") }
            }}),
        );
        paths.insert(
            "/llms.txt".into(),
            json!({ "get": {
                "operationId": "llms", "summary": "How to use the API, for language models",
                "responses": { "200": text("Markdown", "text/plain") }
            }}),
        );
        paths.insert(
            "/robots.txt".into(),
            json!({ "get": {
                "operationId": "robots", "summary": "Crawler rules",
                "responses": { "200": text("robots.txt", "text/plain") }
            }}),
        );
        if self.site {
            paths.insert(
                "/sitemap.xml".into(),
                json!({ "get": {
                    "operationId": "sitemap", "summary": "Sitemap of the page",
                    "responses": { "200": text("XML", "application/xml") }
                }}),
            );
            paths.insert(
                "/docker-compose.yml".into(),
                json!({ "get": {
                    "operationId": "compose", "summary": "Hardened two-container setup",
                    "responses": { "200": text("YAML", "text/plain") }
                }}),
            );
            paths.insert(
                "/config.example.toml".into(),
                json!({ "get": {
                    "operationId": "configExample", "summary": "Every server option, commented",
                    "responses": { "200": text("TOML", "text/plain") }
                }}),
            );
        }
        paths.insert(
            "/healthz".into(),
            json!({ "get": {
                "operationId": "health", "summary": "Liveness",
                "responses": { "200": text("ok", "text/plain") }
            }}),
        );

        let colour = json!({ "$ref": "#/components/headers/Colour" });
        let mut vector: Vec<&str> = VectorOrigin::ALL.iter().map(|v| v.as_str()).collect();
        vector.push("generated");
        let blocked: Vec<&str> = FallbackReason::ALL
            .iter()
            .filter_map(|r| r.blocked_by())
            .collect();
        let spec = json!({
            "openapi": "3.1.0",
            "info": {
                "title": host,
                "version": env!("CARGO_PKG_VERSION"),
                "summary": "Website icons as sanitized SVG, with theme colours in the headers.",
                "description": "One GET returns a site's icon as an edge-to-edge SVG, in colour or as a black monochrome mask. No key, no query parameters, CORS open. The response is an SVG even when the site has no usable icon; the headers explain why.",
                "license": { "name": "MIT", "identifier": "MIT" }
            },
            "servers": [{ "url": self.origin }],
            "externalDocs": {
                "description": "Full HTTP contract",
                "url": "https://github.com/peleda-app/favicons/blob/main/docs/API.md"
            },
            "paths": paths,
            "components": {
                "parameters": {
                    "IfNoneMatch": { "name": "If-None-Match", "in": "header", "required": false, "schema": { "type": "string" } }
                },
                "headers": {
                    "Colour": { "description": "#rrggbb", "schema": { "type": "string", "pattern": "^#[0-9a-f]{6}$" } }
                },
                "responses": {
                    "Icon": {
                        "description": "A real icon, or a generated one when X-Icon-Fallback-Reason is present.",
                        "headers": {
                            "ETag": { "schema": { "type": "string" } },
                            "X-Icon-Source": { "schema": { "type": "string", "enum": names(&Source::ALL, Source::as_str) } },
                            "X-Icon-Domain": { "schema": { "type": "string" } },
                            "X-Icon-Style": { "schema": { "type": "string", "enum": names(&Style::ALL, Style::as_str) } },
                            "X-Icon-Vector": { "schema": { "type": "string", "enum": vector } },
                            "X-Icon-Mono-Source": { "schema": { "type": "string", "enum": names(&MonoSource::ALL, MonoSource::as_str) } },
                            "X-Icon-Provenance": { "schema": { "type": "string", "enum": names(&Provenance::ALL, Provenance::as_str) } },
                            "X-Icon-License": { "schema": { "type": "string" } },
                            "X-Icon-Fallback-Reason": { "schema": { "type": "string", "enum": names(&FallbackReason::ALL, FallbackReason::as_str) } },
                            "X-Icon-Blocked": { "schema": { "type": "string", "enum": blocked } },
                            "X-Icon-Shape": { "schema": { "type": "string", "enum": names(&Shape::ALL, Shape::as_str) } },
                            "X-Icon-Fill": { "schema": { "type": "string", "enum": names(&Fill::ALL, Fill::as_str) } },
                            "X-Icon-Accent": colour,
                            "X-Icon-Light-Plate": colour,
                            "X-Icon-Light-Text": colour,
                            "X-Icon-Light-Mono": colour,
                            "X-Icon-Light-Mono-Plate": colour,
                            "X-Icon-Dark-Plate": colour,
                            "X-Icon-Dark-Text": colour,
                            "X-Icon-Dark-Mono": colour,
                            "X-Icon-Dark-Mono-Plate": colour,
                            "Server-Timing": { "schema": { "type": "string", "examples": ["app;dur=0.83"] } }
                        },
                        "content": { "image/svg+xml": { "schema": { "type": "string" } } }
                    }
                }
            }
        });
        serde_json::to_string_pretty(&spec).unwrap_or_default()
    }
}
