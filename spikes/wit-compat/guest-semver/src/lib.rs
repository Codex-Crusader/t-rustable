wit_bindgen::generate!({ world: "guest", path: "../wit-010" });
use test::api::{types::Point, used::fa};
struct G;
impl Guest for G {
    fn run() -> f64 { fa(Point { x: 1.0, y: 2.0 }) }
}
export!(G);
