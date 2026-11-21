//! Shared plumbing for every Fangst crate above layer 0.

/// The built-in defaults, compiled in so a fresh install runs with no config file.
pub const DEFAULTS_TOML: &str = include_str!("../defaults.toml");

#[cfg(test)]
mod tests {
    use super::DEFAULTS_TOML;

    #[test]
    fn defaults_file_is_compiled_in() {
        assert!(DEFAULTS_TOML.contains("TUNING.md"));
    }
}
