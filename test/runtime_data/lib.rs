use std::path::PathBuf;

fn increment(x: i32) -> i32 {
    x + 1
}

fn runtime_message() -> String {
    let srcdir = std::env::var("TEST_SRCDIR").expect("TEST_SRCDIR should be set");
    let workspace = std::env::var("TEST_WORKSPACE").expect("TEST_WORKSPACE should be set");
    let path = PathBuf::from(srcdir)
        .join(workspace)
        .join("test/runtime_data/message.txt");
    std::fs::read_to_string(path).expect("runtime data file should be readable")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_data_is_available() {
        assert_eq!(runtime_message().trim(), "runtime hello");
    }

    #[test]
    fn basic_logic_still_runs() {
        assert_eq!(increment(1), 2);
    }
}
