pub fn slugify(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut last_was_dash = true;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            for lower in ch.to_lowercase() {
                result.push(lower);
            }
            last_was_dash = false;
        } else if !last_was_dash {
            result.push('-');
            last_was_dash = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_word_lowercases() {
        assert_eq!(slugify("Personal"), "personal");
    }

    #[test]
    fn spaces_become_dashes() {
        assert_eq!(slugify("Acme Work"), "acme-work");
    }

    #[test]
    fn collapses_runs_of_separators() {
        assert_eq!(slugify("foo   bar___baz"), "foo-bar-baz");
    }

    #[test]
    fn strips_unicode_and_punctuation() {
        assert_eq!(slugify("Côté!"), "c-t");
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn trims_trailing_dashes() {
        assert_eq!(slugify("Hello!!!"), "hello");
    }
}
