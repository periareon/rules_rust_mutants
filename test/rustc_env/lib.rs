pub fn has_expected_token() -> bool {
    env!("MUTATION_EXPECTED_TOKEN") == "ok"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustc_env_is_available() {
        assert!(has_expected_token());
    }
}
