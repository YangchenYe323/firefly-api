pub mod bilibili;

#[cfg(test)]
pub fn env() -> std::collections::HashMap<String, String> {
    // Skip in CI
    if std::env::var("CI").is_ok() {
        return std::collections::HashMap::new();
    }

    // Wasm32 has no filesystem access support. Embed the .dev.vars file into the test binary.
    let env = include_str!("../../.dev.vars");
    env.lines().map(|line| {
        let (key, value) = line.split_once('=').unwrap();
        (key.to_string(), value.to_string())
    }).collect()
}