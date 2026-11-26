#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

fuzz_target!(|data: &[u8]| {
    let cfg = config::Render::default();
    if let Ok(v) = render::vectorize(data, &cfg) {
        assert!(vector::verify(&v.svg, cfg.max_output_svg_bytes).is_ok());
    }
});
