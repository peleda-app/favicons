use std::fmt::Write;

use image::RgbaImage;
use visioncortex::color_clusters::{HIERARCHICAL_MAX, KeyingAction, Runner, RunnerConfig};
use visioncortex::{Color, ColorImage, PathSimplifyMode, PointF64};

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub speckle: usize,
    pub color_precision: i32,
    pub layer_difference: i32,
    pub polygon: bool,
    pub corner_deg: i32,
    pub length_threshold: f64,
    pub splice_deg: i32,
}

pub const BALANCED: Params = Params {
    speckle: 8,
    color_precision: 6,
    layer_difference: 16,
    polygon: false,
    corner_deg: 60,
    length_threshold: 4.0,
    splice_deg: 45,
};

pub const FINE: Params = Params {
    speckle: 5,
    color_precision: 7,
    layer_difference: 10,
    polygon: false,
    corner_deg: 60,
    length_threshold: 4.0,
    splice_deg: 45,
};

pub const COARSE: Params = Params {
    speckle: 16,
    color_precision: 4,
    layer_difference: 32,
    polygon: false,
    corner_deg: 90,
    length_threshold: 6.0,
    splice_deg: 45,
};

fn mode(p: &Params) -> PathSimplifyMode {
    if p.polygon {
        PathSimplifyMode::Polygon
    } else {
        PathSimplifyMode::Spline
    }
}

fn rad(deg: i32) -> f64 {
    f64::from(deg).to_radians()
}

fn to_color_image(img: &RgbaImage) -> (ColorImage, Color) {
    let key = [
        [255, 0, 255],
        [0, 255, 0],
        [0, 255, 255],
        [255, 255, 0],
        [1, 2, 3],
        [254, 1, 253],
        [3, 254, 2],
    ]
    .into_iter()
    .find(|k| {
        !img.pixels()
            .any(|p| p[3] >= 128 && p[0] == k[0] && p[1] == k[1] && p[2] == k[2])
    })
    .unwrap_or([255, 0, 255]);
    let keyed = img.pixels().any(|p| p[3] < 128);
    let mut pixels = Vec::with_capacity(img.as_raw().len());
    for p in img.pixels() {
        if p[3] < 128 {
            pixels.extend_from_slice(&[key[0], key[1], key[2], 255]);
        } else {
            pixels.extend_from_slice(&[p[0], p[1], p[2], 255]);
        }
    }
    let ci = ColorImage {
        pixels,
        width: img.width() as usize,
        height: img.height() as usize,
    };
    let key_color = if keyed {
        Color::new(key[0], key[1], key[2])
    } else {
        Color::default()
    };
    (ci, key_color)
}

fn wrap(width: u32, height: u32, body: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">{body}</svg>"#
    )
}

pub fn color(img: &RgbaImage, p: &Params) -> String {
    let (width, height) = (img.width() as usize, img.height() as usize);
    let (ci, key_color) = to_color_image(img);
    let runner = Runner::new(
        RunnerConfig {
            diagonal: p.layer_difference == 0,
            hierarchical: HIERARCHICAL_MAX,
            batch_size: 25_600,
            good_min_area: p.speckle * p.speckle,
            good_max_area: width * height,
            is_same_color_a: 8 - p.color_precision.clamp(1, 8),
            is_same_color_b: 1,
            deepen_diff: p.layer_difference,
            hollow_neighbours: 1,
            key_color,
            keying_action: KeyingAction::Discard,
        },
        ci,
    );
    let clusters = runner.run();
    let view = clusters.view();
    let mut body = String::new();
    for &index in view.clusters_output.iter().rev() {
        let cluster = view.get_cluster(index);
        let path = cluster.to_compound_path(
            &view,
            false,
            mode(p),
            rad(p.corner_deg),
            p.length_threshold,
            10,
            rad(p.splice_deg),
        );
        let (d, offset) = path.to_svg_string(true, PointF64::default(), Some(2));
        let _ = write!(
            body,
            r#"<path d="{d}" fill="{}" transform="translate({},{})"/>"#,
            cluster.residue_color().to_hex_string(),
            offset.x,
            offset.y
        );
    }
    wrap(width as u32, height as u32, &body)
}

pub fn binary(mask: &RgbaImage, p: &Params) -> String {
    let ci = ColorImage {
        pixels: mask
            .pixels()
            .flat_map(|px| {
                if px[3] >= 128 {
                    [0, 0, 0, 255]
                } else {
                    [255, 255, 255, 255]
                }
            })
            .collect(),
        width: mask.width() as usize,
        height: mask.height() as usize,
    };
    let bin = ci.to_binary_image(|c| c.r < 128);
    let clusters = bin.to_clusters(false);
    let mut body = String::new();
    for i in 0..clusters.len() {
        let cluster = clusters.get_cluster(i);
        if cluster.size() < p.speckle * p.speckle {
            continue;
        }
        let path = cluster.to_compound_path(
            mode(p),
            rad(p.corner_deg),
            p.length_threshold,
            10,
            rad(p.splice_deg),
        );
        let (d, offset) = path.to_svg_string(true, PointF64::default(), Some(2));
        let _ = write!(
            body,
            r##"<path d="{d}" fill="#000000" transform="translate({},{})"/>"##,
            offset.x, offset.y
        );
    }
    wrap(mask.width(), mask.height(), &body)
}
