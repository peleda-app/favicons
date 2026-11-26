#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

fuzz_target!(|data: &[u8]| {
    let Some((&pick, bytes)) = data.split_first() else {
        return;
    };
    let source = contract::MonoSource::ALL[usize::from(pick) % contract::MonoSource::ALL.len()];
    let cfg = config::Render::default();
    if let Ok(v) = render::vectorize_mono(bytes, &cfg, source) {
        assert!(vector::verify(&v.svg, cfg.max_output_svg_bytes).is_ok());
    }
});
