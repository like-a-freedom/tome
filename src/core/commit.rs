//! Commit identity helpers shared by output formatting and update logic.

/// Number of characters used to display and compare abbreviated commits.
pub const SHORT_COMMIT_LEN: usize = 7;

/// Return the short form of a commit hash, unchanged when already short.
pub fn short(commit: &str) -> String {
    if commit.len() >= SHORT_COMMIT_LEN {
        commit[..SHORT_COMMIT_LEN].to_string()
    } else {
        commit.to_string()
    }
}

/// Return whether two commits refer to the same revision, allowing one to be
/// the short form of the other.
pub fn matches(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }

    if left.len() > SHORT_COMMIT_LEN && right.len() > SHORT_COMMIT_LEN {
        return false;
    }

    left.starts_with(right) || right.starts_with(left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_truncates_full_hash() {
        assert_eq!(short("abcdef1234567890abcdef"), "abcdef1");
    }

    #[test]
    fn test_short_keeps_short_commit() {
        assert_eq!(short("abc"), "abc");
    }

    #[test]
    fn test_short_exact_boundary() {
        assert_eq!(short("abcdef1"), "abcdef1");
    }

    #[test]
    fn test_matches_full_and_short_hashes() {
        assert!(matches("abcdef1", "abcdef123456"));
        assert!(matches("abcdef1", "abcdef1"));
        assert!(!matches("abcdef123456", "abcdef999999"));
        assert!(!matches("abcdef1", "1234567"));
    }
}
