from __future__ import annotations

import re


FILLER_PATTERN = re.compile(r"\b(?:um+|uh+|er+|ah+|like)\b", re.IGNORECASE)
SPACE_BEFORE_PUNCTUATION = re.compile(r"\s+([,.;:!?])")
MULTISPACE = re.compile(r"\s+")
PARAGRAPH_SPACING = re.compile(r" *\n\n *")

SPOKEN_REPLACEMENTS = {
    "question mark": "?",
    "exclamation mark": "!",
    "comma": ",",
    "period": ".",
    "full stop": ".",
    "colon": ":",
    "semicolon": ";",
    "open bracket": "(",
    "close bracket": ")",
    "open paren": "(",
    "close paren": ")",
    "new paragraph": "\n\n",
}


def cleanup_transcript(text: str) -> str:
    cleaned = f" {text.strip()} "

    for spoken, symbol in sorted(SPOKEN_REPLACEMENTS.items(), key=lambda item: len(item[0]), reverse=True):
        cleaned = re.sub(rf"\b{re.escape(spoken)}\b", f" {symbol} ", cleaned, flags=re.IGNORECASE)

    cleaned = cleaned.replace("\n\n", " __PARAGRAPH_BREAK__ ")
    cleaned = FILLER_PATTERN.sub(" ", cleaned)
    cleaned = MULTISPACE.sub(" ", cleaned).strip()
    cleaned = SPACE_BEFORE_PUNCTUATION.sub(r"\1", cleaned)
    cleaned = cleaned.replace("__PARAGRAPH_BREAK__", "\n\n")
    cleaned = PARAGRAPH_SPACING.sub("\n\n", cleaned)

    if cleaned and cleaned[0].isalpha():
        cleaned = cleaned[0].upper() + cleaned[1:]

    return cleaned
