pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_failure() {
        assert_eq!(add(2, 2), 5);
    }
}
