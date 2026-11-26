#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

fuzz_target!(|data: &[u8]| {
    let _ = render::sniff_svg(data);
    let _ = render::markup_depth(data);
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let deny = vec!["test".to_string()];
    for collapse in [false, true] {
        if let Some(n) = domain::normalize(text, collapse, &deny) {
            assert!(domain::is_valid_hostname(&n.host));
            assert_eq!(domain::normalize(&n.host, collapse, &deny), Some(n.clone()));
            let _ = domain::parent_chain(&n.host).count();
        }
    }
    let _ = domain::is_valid_hostname(text);
    let _ = domain::parent_chain(text).count();
    let _ = policy::parse_blocklist(text).count();
    let _ = discover::parse_sizes(text);
    let _ = palette::Rgb::parse(text);
    let _ = contract::Style::from_path(text);
    let _ = slugs::is_slug(text);
    let _ = slugs::parse_key(text);
    let (a, b) = text.split_once('\n').unwrap_or((text, ""));
    let _ = plate::Plate::parse(a, b);
    let _ = slugs::tint_svg(b.as_bytes(), a);
});
