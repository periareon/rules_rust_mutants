mod untested;

pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn call_untested(x: i32) -> i32 {
    untested::classify(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_is_tested() {
        assert_eq!(add(2, 3), 5);
        assert_eq!(add(-1, 1), 0);
    }
}
