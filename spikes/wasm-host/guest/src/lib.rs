wit_bindgen::generate!({ world: "spike", path: "../wit" });

use spike::api::host;

struct Spike;

impl Guest for Spike {
    fn run(n: u32) -> Result<u32, String> {
        if n == u32::MAX {
            return Err("n is too large".into());
        }
        if n == 0 {
            host::log("run called with 0");
        }
        Ok(n + host::page_count())
    }

    fn spin() {
        let mut i: u64 = 0;
        loop {
            i = std::hint::black_box(i.wrapping_add(1));
        }
    }

    fn grow_memory(mb: u32) -> u32 {
        let mut v: Vec<u8> = Vec::with_capacity(mb as usize * 1024 * 1024);
        v.resize(mb as usize * 1024 * 1024, 1);
        std::hint::black_box(&v);
        (v.len() / (1024 * 1024)) as u32
    }

    fn trap() {
        panic!("guest trap on purpose");
    }
}

export!(Spike);
