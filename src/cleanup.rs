const SPOKEN_REPLACEMENTS: [(&str, &str); 11] = [
    ("question mark", "?"),
    ("exclamation mark", "!"),
    ("full stop", "."),
    ("semicolon", ";"),
    ("open bracket", "("),
    ("close bracket", ")"),
    ("open paren", "("),
    ("close paren", ")"),
    ("new paragraph", "\n\n"),
    ("period", "."),
    ("comma", ","),
];

pub fn cleanup_transcript(text: &str) -> String {
    let mut cleaned = format!(" {} ", text.trim());

    for (spoken, symbol) in SPOKEN_REPLACEMENTS {
        cleaned = replace_phrase_case_insensitive(&cleaned, spoken, &format!(" {} ", symbol));
    }

    cleaned = cleaned.replace("\n\n", " __PARAGRAPH_BREAK__ ");
    cleaned = remove_fillers(&cleaned);
    cleaned = collapse_whitespace(&cleaned);
    cleaned = trim_space_before_punctuation(&cleaned);
    cleaned = cleaned.replace("__PARAGRAPH_BREAK__", "\n\n");
    cleaned = normalize_paragraph_spacing(&cleaned);

    let mut chars = cleaned.chars();
    match chars.next() {
        Some(first) if first.is_alphabetic() => {
            let mut capitalized = first.to_uppercase().collect::<String>();
            capitalized.push_str(chars.as_str());
            capitalized
        }
        _ => cleaned,
    }
}

fn replace_phrase_case_insensitive(input: &str, needle: &str, replacement: &str) -> String {
    let lower_input = input.to_lowercase();
    let lower_needle = needle.to_lowercase();
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(relative_start) = lower_input[cursor..].find(&lower_needle) {
        let start = cursor + relative_start;
        let end = start + lower_needle.len();

        if !is_word_boundary(&lower_input, start, end) {
            output.push_str(&input[cursor..end]);
            cursor = end;
            continue;
        }

        output.push_str(&input[cursor..start]);
        output.push_str(replacement);
        cursor = end;
    }

    output.push_str(&input[cursor..]);
    output
}

fn remove_fillers(input: &str) -> String {
    input
        .split_whitespace()
        .filter(|token| !is_filler(token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_filler(token: &str) -> bool {
    let trimmed = token.trim_matches(|ch: char| !ch.is_alphanumeric());
    if trimmed.is_empty() {
        return false;
    }

    match trimmed.to_ascii_lowercase().as_str() {
        token if repeated_letter_word(token, 'u', 'm') => true,
        token if repeated_letter_word(token, 'u', 'h') => true,
        token if repeated_letter_word(token, 'e', 'r') => true,
        token if repeated_letter_word(token, 'a', 'h') => true,
        _ => false,
    }
}

fn repeated_letter_word(token: &str, first: char, repeat: char) -> bool {
    let mut chars = token.chars();
    matches!(chars.next(), Some(ch) if ch == first) && chars.all(|ch| ch == repeat)
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn trim_space_before_punctuation(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut pending_space = false;

    for ch in input.chars() {
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }

        if pending_space && !matches!(ch, ',' | '.' | ';' | ':' | '!' | '?') {
            output.push(' ');
        }

        output.push(ch);
        pending_space = false;
    }

    output.trim().to_string()
}

fn normalize_paragraph_spacing(input: &str) -> String {
    let mut normalized = String::new();
    let mut parts = input.split("\n\n").peekable();

    while let Some(part) = parts.next() {
        normalized.push_str(part.trim());
        if parts.peek().is_some() {
            normalized.push_str("\n\n");
        }
    }

    normalized.trim().to_string()
}

fn is_word_boundary(input: &str, start: usize, end: usize) -> bool {
    let before = input[..start].chars().next_back();
    let after = input[end..].chars().next();
    !matches!(before, Some(ch) if ch.is_alphanumeric())
        && !matches!(after, Some(ch) if ch.is_alphanumeric())
}

