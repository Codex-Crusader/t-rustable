//! The `.tmod` reader on random bytes. It must never panic, and each name that it
//! accepts must pass the name rules.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(package) = t_rustable_tmod::read(data) {
        for path in package.files.keys() {
            assert!(t_rustable_tmod::names::check_path(path).is_ok());
        }
    }
});
