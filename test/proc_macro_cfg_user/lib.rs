use proc_macro_cfg_macro::generate_cfg_value;

generate_cfg_value!();

fn is_nonzero(x: i32) -> bool {
    x != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_macro_generated_code_respects_cfg() {
        let expected = if cfg!(mutation_proc_macro_enabled) {
            "enabled"
        } else {
            "disabled"
        };

        assert_eq!(generated_cfg_value(), expected);
    }

    #[test]
    fn basic_logic_still_runs() {
        assert!(is_nonzero(1));
        assert!(!is_nonzero(0));
    }
}
