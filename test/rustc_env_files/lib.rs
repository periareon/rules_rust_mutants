pub fn has_expected_token() -> bool {
    env!("MUTATION_EXPECTED_TOKEN_FROM_FILE") == "ok_from_file"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustc_env_file_is_available() {
        assert!(has_expected_token());
    }
}
