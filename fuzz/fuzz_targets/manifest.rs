#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

fuzz_target!(|data: &[u8]| {
    let scope = fuzz_scope();
    let job = fuzz_job(sandbox::Task::Manifest {
        url: "https://www.example.com/app/manifest.webmanifest".into(),
        scope: scope.clone(),
    });
    let result = sandbox::execute(&job, data);
    let wire = sandbox::encode_response(&result);
    if let Ok(sandbox::Output::Page(page)) = result {
        let back = sandbox::decode_page(&wire, &scope).expect("own response decodes");
        assert_eq!(back, page);
    }
});
