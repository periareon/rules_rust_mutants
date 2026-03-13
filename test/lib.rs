fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn is_positive(x: i32) -> bool {
    x > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(2, 3), 5);
        assert_eq!(add(0, 0), 0);
        assert_eq!(add(-1, 1), 0);
    }

    #[test]
    fn test_is_positive() {
        assert_eq!(is_positive(1), true);
        assert_eq!(is_positive(-1), false);
        assert_eq!(is_positive(0), false);
    }
}
