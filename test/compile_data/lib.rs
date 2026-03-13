pub fn has_expected_message_len() -> bool {
    include_str!("message.txt").trim() == "hello"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_data_is_available() {
        assert!(has_expected_message_len());
    }
}
