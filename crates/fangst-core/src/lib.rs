//! Shared plumbing for every Fangst crate above layer 0.

pub mod config;
pub mod defaults;
pub mod folders;
pub mod path;
pub mod secret;

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
