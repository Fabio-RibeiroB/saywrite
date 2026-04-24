use saywrite::cleanup::cleanup_transcript;

#[test]
fn test_spoken_replacements() {
    // Spoken punctuation marks are replaced
    let result = cleanup_transcript("what is the question mark");
    assert!(result.contains("?"));

    let result = cleanup_transcript("hello exclamation mark");
    assert!(result.contains("!"));

    let result = cleanup_transcript("end full stop");
    assert!(result.contains("."));
}

#[test]
fn test_filler_removal() {
    // um, uh, er, ah should be removed
    assert_eq!(cleanup_transcript("this um is good"), "This is good");
    assert_eq!(cleanup_transcript("well uh maybe"), "Well maybe");
    assert_eq!(cleanup_transcript("i er think so"), "I think so");
    assert_eq!(cleanup_transcript("ah yes"), "Yes");
}

#[test]
fn test_whitespace_collapse() {
    assert_eq!(cleanup_transcript("hello    world"), "Hello world");
    assert_eq!(cleanup_transcript("   hello world   "), "Hello world");
}

#[test]
fn test_punctuation_spacing() {
    assert_eq!(cleanup_transcript("hello , world"), "Hello, world");
    assert_eq!(cleanup_transcript("what ?"), "What?");
}

#[test]
fn test_punctuation_run_collapse() {
    assert_eq!(
        cleanup_transcript("testing again, period"),
        "Testing again."
    );
    assert_eq!(cleanup_transcript("wait question mark period"), "Wait?");
}

#[test]
fn test_capitalization() {
    // First letter capitalized if alphabetic
    let result = cleanup_transcript("hello world");
    assert!(result.starts_with("H"));

    // All caps stays as-is (only first letter processed)
    let result = cleanup_transcript("HELLO WORLD");
    assert!(result.starts_with("H"));
}

#[test]
fn test_paragraph_breaks() {
    // "new paragraph" becomes paragraph break
    let result = cleanup_transcript("hello new paragraph world");
    assert!(result.contains("\n\n"));
}

#[test]
fn test_empty_input() {
    assert_eq!(cleanup_transcript(""), "");
    assert_eq!(cleanup_transcript("   "), "");
}

#[test]
fn test_non_alphabetic_start() {
    // Numbers at start don't trigger capitalization
    let result = cleanup_transcript("123 hello");
    assert!(result.starts_with("1"));
}

#[test]
fn test_combined() {
    let input = "so um i think what is the question mark";
    let result = cleanup_transcript(input);
    // Should remove "um", clean up punctuation, collapse whitespace, capitalize
    assert!(result.starts_with("So"));
    assert!(result.contains("?"));
    assert!(!result.contains("um"));
}
