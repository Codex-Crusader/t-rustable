//! Tauri shell for the T-Rustable GUI. Work starts in Milestone 4.

fn banner() -> String {
    format!("T-Rustable {}", t_rustable_core::VERSION)
}

fn main() {
    println!("{}", banner());
}

#[cfg(test)]
mod tests {
    #[test]
    fn banner_names_the_program() {
        assert!(super::banner().starts_with("T-Rustable "));
    }
}
