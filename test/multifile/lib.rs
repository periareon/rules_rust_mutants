mod math;

pub fn is_positive(x: i32) -> bool {
    x > 0
}

pub fn add_if_positive(a: i32, b: i32) -> i32 {
    if is_positive(a) {
        math::add(a, b)
    } else {
        a
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_if_positive() {
        assert_eq!(add_if_positive(2, 3), 5);
        assert_eq!(add_if_positive(-2, 3), -2);
    }

    #[test]
    fn test_is_positive() {
        assert_eq!(is_positive(1), true);
        assert_eq!(is_positive(-1), false);
        assert_eq!(is_positive(0), false);
    }
}
