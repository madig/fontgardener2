/// Transform a name such that it can be written to case-preserving but case-insensitive
/// filesystems without overwriting something else.
pub fn name_to_filename(name: &str) -> String {
    let mut filename = String::new();
    for c in name.chars() {
        if c.is_uppercase() {
            filename.push(c);
            filename.push('_');
        } else if c == '_' {
            filename.push_str("__")
        } else {
            filename.push(c);
        }
    }
    filename
}

/// Transform a filename from a case-preserving but case-insensitive such that we arrive
/// at the previously intended name.
pub fn filename_to_name(filename: &str) -> String {
    let mut name = String::new();
    let mut previous_char_was_uppercase = false;
    let mut previous_char_was_underscore = false;
    for c in filename.chars() {
        if c == '_' && (previous_char_was_uppercase || previous_char_was_underscore) {
            // Suppress underscores if the previous char was an uppercase character or
            // an (escaped) underscore, because we already pushed it and don't need to
            // do anything else. Reset the counters to start afresh.
            previous_char_was_uppercase = false;
            previous_char_was_underscore = false;
            continue;
        }
        name.push(c);
        previous_char_was_uppercase = c.is_uppercase();
        previous_char_was_underscore = c == '_';
    }
    name
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        /// Test that we can roundtrip glyph names, at least before hitting the
        /// filesystem.
        #[test]
        fn roundtrip(name in "\\P{Other}*") {
            assert_eq!(name, filename_to_name(&name_to_filename(&name)));
        }
    }

    proptest! {
        /// Test that our escaping of uppercase characters and underscores does not
        /// clash with unluckily named lowercase names, like the filename for "A"
        /// clashing with the one for "a_" on case-insensitive filesystems.
        #[test]
        fn distinct_upper_lower_case(name in "\\P{Other}+") {
            // TODO: Find way to more efficiently generate only strings that are upper
            // or mixed case, but not all lowercase (because we lowercase ourselves and
            // are not interested in comparing identical names). Or find another way to
            // test clash avoidance.
            let lowercased = name.to_lowercase();
            let uppercased = if name == lowercased {
                name.to_uppercase()
            } else {
                name
            };

            prop_assume!(uppercased != lowercased);
            assert_ne!(name_to_filename(&lowercased), name_to_filename(&uppercased));
        }
    }
}
