fn is_nonzero(x: i32) -> bool {
    x != 0
}

fn compile_time_env_is_scrubbed() -> bool {
    option_env!("MUTATION_COMPILE_LEAK").is_none()
}

fn runtime_env_is_scrubbed() -> bool {
    std::env::var("MUTATION_RUNTIME_LEAK").is_err()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_env_does_not_leak() {
        assert!(compile_time_env_is_scrubbed());
    }

    #[test]
    fn runtime_env_does_not_leak() {
        assert!(runtime_env_is_scrubbed());
    }

    #[test]
    fn basic_logic_still_runs() {
        assert!(is_nonzero(1));
        assert!(!is_nonzero(0));
    }
}
