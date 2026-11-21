use http::{HeaderName, HeaderValue};

use crate::contract::Style;

const SIDE: f64 = 128.0;
const INSET: f64 = 0.18;
const ROUNDED: &str = "M28.8 0h70.4A28.8 28.8 0 0 1 128 28.8v70.4A28.8 28.8 0 0 1 99.2 128H28.8A28.8 28.8 0 0 1 0 99.2V28.8A28.8 28.8 0 0 1 28.8 0Z";
const SUPERELLIPSE: &str = "M127.89 49.83Q128 64 127.89 78.17Q127.78 92.34 127.45 96.81Q127.12 101.27 126.56 104.43Q126 107.58 125.21 110.04Q124.42 112.5 123.38 114.49Q122.34 116.48 121.03 118.1Q119.72 119.72 118.1 121.03Q116.48 122.34 114.49 123.38Q112.5 124.42 110.04 125.21Q107.58 126 104.43 126.56Q101.27 127.12 96.81 127.45Q92.34 127.78 78.17 127.89Q64 128 49.83 127.89Q35.66 127.78 31.19 127.45Q26.73 127.12 23.57 126.56Q20.42 126 17.96 125.21Q15.5 124.42 13.51 123.38Q11.52 122.34 9.9 121.03Q8.28 119.72 6.97 118.1Q5.66 116.48 4.62 114.49Q3.58 112.5 2.79 110.04Q2 107.58 1.44 104.43Q0.88 101.27 0.55 96.81Q0.22 92.34 0.11 78.17Q0 64 0.11 49.83Q0.22 35.66 0.55 31.19Q0.88 26.73 1.44 23.57Q2 20.42 2.79 17.96Q3.58 15.5 4.62 13.51Q5.66 11.52 6.97 9.9Q8.28 8.28 9.9 6.97Q11.52 5.66 13.51 4.62Q15.5 3.58 17.96 2.79Q20.42 2 23.57 1.44Q26.73 0.88 31.19 0.55Q35.66 0.22 49.83 0.11Q64 0 78.17 0.11Q92.34 0.22 96.81 0.55Q101.27 0.88 104.43 1.44Q107.58 2 110.04 2.79Q112.5 3.58 114.49 4.62Q116.48 5.66 118.1 6.97Q119.72 8.28 121.03 9.9Q122.34 11.52 123.38 13.51Q124.42 15.5 125.21 17.96Q126 20.42 126.56 23.57Q127.12 26.73 127.45 31.19Q127.78 35.66 127.89 49.83Z";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rounded,
    Superellipse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plate {
    pub shape: Shape,
    pub theme: Theme,
}

impl Plate {
    pub const SHAPES: [&'static str; 2] = ["rounded", "superellipse"];
    pub const THEMES: [&'static str; 2] = ["light", "dark"];

    pub fn parse(shape: &str, theme: &str) -> Option<Self> {
        let shape = match shape {
            "rounded" => Shape::Rounded,
            "superellipse" => Shape::Superellipse,
            _ => return None,
        };
        let theme = match theme {
            "light" => Theme::Light,
            "dark" => Theme::Dark,
            _ => return None,
        };
        Some(Self { shape, theme })
    }

    pub fn suffix(self) -> &'static str {
        match (self.shape, self.theme) {
            (Shape::Rounded, Theme::Light) => "/rounded/light",
            (Shape::Rounded, Theme::Dark) => "/rounded/dark",
            (Shape::Superellipse, Theme::Light) => "/superellipse/light",
            (Shape::Superellipse, Theme::Dark) => "/superellipse/dark",
        }
    }

    fn path(self) -> &'static str {
        match self.shape {
            Shape::Rounded => ROUNDED,
            Shape::Superellipse => SUPERELLIPSE,
        }
    }
}

fn header<'a>(palette: &'a [(HeaderName, HeaderValue)], name: &str) -> Option<&'a str> {
    palette
        .iter()
        .find(|(n, _)| n.as_str() == name)
        .and_then(|(_, v)| v.to_str().ok())
}

fn colour<'a>(palette: &'a [(HeaderName, HeaderValue)], name: &str, default: &'a str) -> &'a str {
    header(palette, name)
        .filter(|c| {
            c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
        })
        .unwrap_or(default)
}

fn split_svg(svg: &str) -> Option<(&str, &str)> {
    let open_end = svg.find('>')?;
    let open = svg.get(..open_end)?;
    if !open.starts_with("<svg") || open.ends_with('/') {
        return None;
    }
    let view_box = open
        .split("viewBox=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())?;
    let inner = svg.get(open_end + 1..)?.trim_end().strip_suffix("</svg>")?;
    Some((view_box, inner))
}

pub fn compose(
    svg: &[u8],
    plate: Plate,
    style: Style,
    palette: &[(HeaderName, HeaderValue)],
) -> Option<Vec<u8>> {
    let svg = std::str::from_utf8(svg).ok()?;
    let (view_box, inner) = split_svg(svg)?;
    let theme = match plate.theme {
        Theme::Light => "light",
        Theme::Dark => "dark",
    };
    let (default_plate, default_mono, default_mono_plate) = match plate.theme {
        Theme::Light => ("#f2f2f4", "#3a3a3c", "#ececf0"),
        Theme::Dark => ("#2c2c2e", "#d4d4d8", "#2c2c2e"),
    };
    let bleed = header(palette, "x-icon-fill") == Some("bleed");
    let (fill, body) = match style {
        Style::Color => (
            colour(palette, &format!("x-icon-{theme}-plate"), default_plate),
            inner.to_string(),
        ),
        Style::Mono => {
            let mono = colour(palette, &format!("x-icon-{theme}-mono"), default_mono);
            (
                colour(
                    palette,
                    &format!("x-icon-{theme}-mono-plate"),
                    default_mono_plate,
                ),
                inner
                    .replace("fill=\"#000000\"", &format!("fill=\"{mono}\""))
                    .replace("fill=\"#000\"", &format!("fill=\"{mono}\"")),
            )
        }
    };
    let (offset, size) = if bleed && style == Style::Color {
        (0.0, SIDE)
    } else {
        (SIDE * INSET, SIDE * (1.0 - 2.0 * INSET))
    };
    let path = plate.path();
    let out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"128\" height=\"128\" viewBox=\"0 0 128 128\">\
         <defs><clipPath id=\"plate\"><path d=\"{path}\"/></clipPath></defs>\
         <path d=\"{path}\" fill=\"{fill}\"/>\
         <g clip-path=\"url(#plate)\"><svg x=\"{offset:.2}\" y=\"{offset:.2}\" width=\"{size:.2}\" height=\"{size:.2}\" viewBox=\"{view_box}\">{body}</svg></g></svg>"
    );
    Some(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Vec<(HeaderName, HeaderValue)> {
        [
            ("x-icon-fill", "inset"),
            ("x-icon-light-plate", "#ebecfa"),
            ("x-icon-dark-plate", "#181b3f"),
            ("x-icon-light-mono", "#5a67f2"),
            ("x-icon-light-mono-plate", "#e7e9f8"),
            ("x-icon-dark-mono", "#6b77f3"),
            ("x-icon-dark-mono-plate", "#1d2044"),
        ]
        .into_iter()
        .map(|(n, v)| (HeaderName::from_static(n), HeaderValue::from_static(v)))
        .collect()
    }

    const ICON: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><path fill="#000000" d="M0 0h128v128H0Z"/></svg>"##;

    #[test]
    fn puts_the_icon_on_a_themed_plate() {
        let out = compose(
            ICON.as_bytes(),
            Plate::parse("rounded", "dark").unwrap(),
            Style::Color,
            &palette(),
        )
        .unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("fill=\"#181b3f\""));
        assert!(out.contains("x=\"23.04\""));
        assert!(out.contains("viewBox=\"0 0 128 128\"><path fill=\"#000000\""));
        assert!(out.ends_with("</svg></g></svg>"));
    }

    #[test]
    fn tints_the_mask_and_uses_the_mono_plate() {
        let out = compose(
            ICON.as_bytes(),
            Plate::parse("superellipse", "light").unwrap(),
            Style::Mono,
            &palette(),
        )
        .unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("fill=\"#e7e9f8\""));
        assert!(out.contains("fill=\"#5a67f2\" d=\"M0 0h128"));
        assert!(!out.contains("#000000"));
        assert!(out.contains(SUPERELLIPSE));
    }

    #[test]
    fn bleed_icons_fill_the_plate_and_bad_colours_fall_back() {
        let mut p = palette();
        p[0].1 = HeaderValue::from_static("bleed");
        p[2].1 = HeaderValue::from_static("red\"/><script>");
        let out = compose(
            ICON.as_bytes(),
            Plate::parse("rounded", "dark").unwrap(),
            Style::Color,
            &p,
        )
        .unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("x=\"0.00\" y=\"0.00\" width=\"128.00\""));
        assert!(!out.contains("script"));
        assert!(out.contains("fill=\"#2c2c2e\""));
    }

    #[test]
    fn parses_only_known_shapes_and_themes() {
        assert!(Plate::parse("circle", "dark").is_none());
        assert!(Plate::parse("rounded", "blue").is_none());
        assert_eq!(
            Plate::parse("superellipse", "light").unwrap().suffix(),
            "/superellipse/light"
        );
        assert!(
            compose(
                b"not svg",
                Plate::parse("rounded", "dark").unwrap(),
                Style::Color,
                &palette()
            )
            .is_none()
        );
    }
}
