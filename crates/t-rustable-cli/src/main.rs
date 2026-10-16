//! Command line front end. Work starts in Milestone 6.

fn banner() -> String {
    format!("t-rustable {}", t_rustable_core::VERSION)
}

fn main() {
    println!("{}", banner());
}

#[cfg(test)]
mod tests {
    #[test]
    fn banner_names_the_program() {
        assert!(super::banner().starts_with("t-rustable "));
    }
}
