#![no_main]
#![allow(dead_code, unused_imports)]

use libfuzzer_sys::fuzz_target;

include!("modules.rs");

fuzz_target!(|data: &[u8]| {
    if let Ok((job, payload)) = sandbox::decode_request(data) {
        let again = sandbox::encode_request(&job, payload).expect("decoded request re-encodes");
        assert!(sandbox::decode_request(&again).is_ok());
    }
});
