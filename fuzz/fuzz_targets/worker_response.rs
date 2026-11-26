#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

fuzz_target!(|data: &[u8]| {
    let cfg = config::Render::default();
    if let Ok(v) = sandbox::decode_vector(data, cfg.max_output_svg_bytes) {
        assert!(v.svg.len() <= cfg.max_output_svg_bytes * 2);
    }
    let _ = sandbox::decode_palette(data);
    let scope = fuzz_scope();
    if let Ok(page) = sandbox::decode_page(data, &scope) {
        for c in &page.icons {
            match &c.location {
                discover::Location::Remote(u) => assert!(scope.allows(u)),
                discover::Location::Inline(b) => assert!(b.len() <= scope.max_inline),
            }
        }
        if let Some(m) = &page.manifest {
            assert!(scope.allows(m));
        }
    }
});
