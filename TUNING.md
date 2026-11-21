# Tuning

Every value Fangst uses that you might want to change: connection counts, timeouts, retry backoff, seeding limits and the like. The built-in defaults live in `crates/fangst-core/defaults.toml`; your own config file overrides any of them.

Each entry says what the value does, its default, its allowed range, and when you'd change it. Entries are added together with the code that reads them.

No values yet.
