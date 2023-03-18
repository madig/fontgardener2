/// Transform a name such that it can be written to case-preserving but case-insensitive
/// filesystems without overwriting something else.
pub fn name_to_filename(name: &str) -> String {
    let mut filename = String::new();
    for c in name.chars() {
        if c.is_uppercase() {
            filename.push(c);
            filename.push('_');
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
    for c in filename.chars() {
        if c == '_' && previous_char_was_uppercase {
            continue;
        }
        name.push(c);
        previous_char_was_uppercase = c.is_uppercase()
    }
    name
}
