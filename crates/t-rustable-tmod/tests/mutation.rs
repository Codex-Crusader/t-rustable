//! Stable fuzz test: random changes to good packages must never panic or pass a bad
//! name. The cargo-fuzz target in `fuzz/` runs the same reader with libFuzzer in CI.

use std::collections::BTreeMap;

use t_rustable_tmod::{names, read, write};

/// xorshift64, so each run tests the same cases.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

fn seed() -> Vec<u8> {
    let manifest = r#"{"id":"com.example.fuzz","name":"Fuzz","version":"1.0.0","apiVersion":"0.1","author":"A","description":"D","permissions":["document.read","ui"],"entry":"m.wasm","worlds":["interactive"],"panels":[{"id":"p","title":"P","entry":"ui/p.html"}]}"#;
    let files: BTreeMap<String, Vec<u8>> = [
        ("manifest.json", manifest.as_bytes().to_vec()),
        ("m.wasm", vec![0, 97, 115, 109, 13, 0, 1, 0]),
        ("ui/p.html", b"<!doctype html><p>panel</p>".repeat(20)),
        ("ui/p.js", b"let x = 1;".repeat(50)),
    ]
    .into_iter()
    .map(|(n, b)| (n.to_string(), b))
    .collect();
    write(&files).unwrap()
}

#[test]
fn random_changes_never_panic() {
    let good = seed();
    assert!(read(&good).is_ok());
    let mut rng = Rng(0x7275_7374_6162_6c65);
    let mut accepted = 0;
    for _ in 0..5000 {
        let mut bytes = good.clone();
        for _ in 0..1 + rng.below(8) {
            match rng.below(5) {
                0 => {
                    let i = rng.below(bytes.len());
                    bytes[i] ^= 1 << rng.below(8);
                }
                1 => {
                    let i = rng.below(bytes.len());
                    bytes[i] = rng.next() as u8;
                }
                2 => {
                    let n = rng.below(bytes.len());
                    bytes.truncate(n);
                }
                3 => {
                    let i = rng.below(bytes.len() + 1);
                    let b = rng.next() as u8;
                    bytes.insert(i.min(bytes.len()), b);
                }
                _ => {
                    // Copy a block over another place, which moves headers around.
                    let len = 1 + rng.below(64);
                    let from = rng.below(bytes.len().saturating_sub(len));
                    let to = rng.below(bytes.len().saturating_sub(len));
                    if from + len <= bytes.len() && to + len <= bytes.len() {
                        let block = bytes[from..from + len].to_vec();
                        bytes[to..to + len].copy_from_slice(&block);
                    }
                }
            }
            if bytes.is_empty() {
                break;
            }
        }
        if let Ok(p) = read(&bytes) {
            accepted += 1;
            for path in p.files.keys() {
                assert_eq!(names::check_path(path), Ok(()), "{path:?}");
            }
        }
    }
    // Some changes hit only the file data, so a few packages still pass.
    assert!(accepted < 5000);
}
