pub fn load_policy() -> String {
    std::fs::read_to_string("policy.toml").unwrap_or_default()
}
