wit_bindgen::generate!({ world: "guest", path: "../wit-v1" });
use test::api::edit::{submit, Command};
struct G;
impl Guest for G {
    fn run() -> u32 { submit(&[Command::A(1), Command::B("x".into())]) }
}
export!(G);
