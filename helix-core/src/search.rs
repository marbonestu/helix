use crate::movement::Direction;
use crate::RopeSlice;
use std::ops::Range;

// TODO: switch to std::str::Pattern when it is stable.
pub trait CharMatcher {
    fn char_match(&self, ch: char) -> bool;
}

impl CharMatcher for char {
    fn char_match(&self, ch: char) -> bool {
        *self == ch
    }
}

impl<F: Fn(&char) -> bool> CharMatcher for F {
    fn char_match(&self, ch: char) -> bool {
        (*self)(&ch)
    }
}

// Finds the positions of the nth matching character in given direction
// starting from the pos gap-index (see Range struct for explanation)
pub fn find_nth_char<M: CharMatcher>(
    mut n: usize,
    text: RopeSlice,
    char_matcher: M,
    mut pos: usize,
    direction: Direction,
) -> Option<usize> {
    if n == 0 {
        return None;
    }

    let mut chars = text.get_chars_at(pos)?;

    match direction {
        Direction::Forward => loop {
            let c = chars.next()?;
            if char_matcher.char_match(c) {
                n -= 1;
                if n == 0 {
                    return Some(pos);
                }
            }
            pos += 1;
        },
        Direction::Backward => loop {
            let c = chars.prev()?;
            pos -= 1;
            if char_matcher.char_match(c) {
                n -= 1;
                if n == 0 {
                    return Some(pos);
                }
            }
        },
    };
}

/// Collect all char positions in `range` where `ch` appears.
/// When `case_insensitive` is true, matching ignores case via Unicode lowercasing.
pub fn find_all_char_matches(
    text: RopeSlice,
    ch: char,
    range: Range<usize>,
    case_insensitive: bool,
) -> Vec<usize> {
    let mut results = Vec::new();
    let mut pos = range.start;
    for c in text.chars_at(range.start) {
        if pos >= range.end {
            break;
        }
        let is_match = if case_insensitive {
            c.to_lowercase().eq(ch.to_lowercase())
        } else {
            c == ch
        };
        if is_match {
            results.push(pos);
        }
        pos += 1;
    }
    results
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::movement::Direction;

    #[test]
    fn test_find_nth_char() {
        let text = RopeSlice::from("aa ⌚aa \r\n aa");

        // Forward direction
        assert_eq!(find_nth_char(1, text, 'a', 5, Direction::Forward), Some(5));
        assert_eq!(find_nth_char(2, text, 'a', 5, Direction::Forward), Some(10));
        assert_eq!(find_nth_char(3, text, 'a', 5, Direction::Forward), Some(11));
        assert_eq!(find_nth_char(4, text, 'a', 5, Direction::Forward), None);

        // Backward direction
        assert_eq!(find_nth_char(1, text, 'a', 5, Direction::Backward), Some(4));
        assert_eq!(find_nth_char(2, text, 'a', 5, Direction::Backward), Some(1));
        assert_eq!(find_nth_char(3, text, 'a', 5, Direction::Backward), Some(0));
        assert_eq!(find_nth_char(4, text, 'a', 5, Direction::Backward), None);

        // Edge cases
        assert_eq!(find_nth_char(0, text, 'a', 5, Direction::Forward), None); // n = 0
        assert_eq!(find_nth_char(1, text, 'x', 5, Direction::Forward), None); // Not found
        assert_eq!(find_nth_char(1, text, 'a', 20, Direction::Forward), None); // Beyond text
        assert_eq!(find_nth_char(1, text, 'a', 0, Direction::Backward), None); // At start going backward
    }

    #[test]
    fn test_find_all_char_matches() {
        use crate::Rope;

        let text = Rope::from("abcabc");
        let slice = text.slice(..);

        // Find all 'a' in full range
        assert_eq!(find_all_char_matches(slice, 'a', 0..6, false), vec![0, 3]);

        // Find all 'b' in full range
        assert_eq!(find_all_char_matches(slice, 'b', 0..6, false), vec![1, 4]);

        // Find in sub-range
        assert_eq!(find_all_char_matches(slice, 'a', 1..6, false), vec![3]);

        // No matches
        assert_eq!(
            find_all_char_matches(slice, 'z', 0..6, false),
            Vec::<usize>::new()
        );

        // Empty range
        assert_eq!(
            find_all_char_matches(slice, 'a', 0..0, false),
            Vec::<usize>::new()
        );

        // Unicode
        let text = Rope::from("a⌚a⌚a");
        let slice = text.slice(..);
        assert_eq!(find_all_char_matches(slice, '⌚', 0..5, false), vec![1, 3]);
        assert_eq!(find_all_char_matches(slice, 'a', 0..5, false), vec![0, 2, 4]);
    }

    #[test]
    fn test_find_all_char_matches_case_insensitive() {
        use crate::Rope;

        let text = Rope::from("AbCaBc");
        let slice = text.slice(..);

        // Case-insensitive: lowercase query matches both cases
        assert_eq!(find_all_char_matches(slice, 'a', 0..6, true), vec![0, 3]);
        assert_eq!(find_all_char_matches(slice, 'A', 0..6, true), vec![0, 3]);
        assert_eq!(find_all_char_matches(slice, 'c', 0..6, true), vec![2, 5]);

        // Case-sensitive: only exact matches
        assert_eq!(find_all_char_matches(slice, 'a', 0..6, false), vec![3]);
        assert_eq!(find_all_char_matches(slice, 'A', 0..6, false), vec![0]);
    }
}
