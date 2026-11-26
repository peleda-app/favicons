#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

fuzz_target!(|data: &[u8]| {
    let job = fuzz_job(sandbox::Task::Analyze);
    let _ = sandbox::execute(&job, data);
    if let Ok(svg) = std::str::from_utf8(data)
        && vector::verify(svg, job.cfg.max_output_svg_bytes).is_ok()
    {
        let _ = vector::scope(svg);
    }
});
