#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

const FORMATS: [config::IndexFormat; 6] = [
    config::IndexFormat::Selfhst,
    config::IndexFormat::SimpleIcons,
    config::IndexFormat::DashboardIcons,
    config::IndexFormat::Devicon,
    config::IndexFormat::Thesvg,
    config::IndexFormat::Iconify,
];

fuzz_target!(|data: &[u8]| {
    let Some((&pick, json)) = data.split_first() else {
        return;
    };
    let format = FORMATS[usize::from(pick) % FORMATS.len()];
    let licenses = vec!["MIT".to_string(), "CC0-1.0".to_string()];
    if let Ok(items) = slugs::parse_index(format, json, &licenses) {
        for slug in items.keys() {
            assert!(slugs::is_slug(slug));
        }
    }
});
