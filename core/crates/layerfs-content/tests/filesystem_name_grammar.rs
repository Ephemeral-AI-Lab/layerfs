//! The name grammar accepts well-formed UTF-8 and the canonical WTF-8
//! spelling of an unpaired surrogate (a Windows host name); a pair spelled
//! as two surrogate sequences and every broken encoding are refused, so
//! one name has one byte spelling on every platform.
use layerfs_content::PathName;

#[test]
fn utf8_names_and_unpaired_surrogates_are_accepted() {
    for bytes in [
        "ascii.txt".as_bytes(),
        "中文".as_bytes(),
        // The supplementary plane keeps its UTF-8 scalar spelling.
        "🚀-🧬".as_bytes(),
        // An unpaired high (U+D800) and an unpaired low (U+DC80) surrogate,
        // each in its three-byte WTF-8 form.
        &[0xED, 0xA0, 0x80][..],
        &[0xED, 0xB2, 0x80][..],
        // An unpaired surrogate beside ordinary text.
        &[b'A', 0xED, 0xA0, 0x80, b'B'][..],
        // Two unpaired high surrogates in a row are each unpaired.
        &[0xED, 0xA0, 0x80, 0xED, 0xA0, 0xBF][..],
    ] {
        assert!(PathName::from_bytes(bytes).is_ok(), "{bytes:02x?}");
    }
}

#[test]
fn split_pairs_and_broken_encodings_are_refused() {
    for bytes in [
        // 🚀 (U+1F680) spelled as D83D DE80 in two surrogate sequences.
        &[0xED, 0xA0, 0xBD, 0xED, 0xBA, 0x80][..],
        // Overlong encodings of '/' and 'A'.
        &[0xE0, 0x80, 0xAF][..],
        &[0xC0, 0xAF][..],
        // Beyond U+10FFFF and a bare continuation byte.
        &[0xF5, 0x80, 0x80, 0x80][..],
        &[0x80][..],
        // A truncated four-byte tail.
        &[0xF0, 0x9F][..],
        // A malformed continuation.
        &[0xE4, 0xB8, 0x00][..],
    ] {
        assert!(PathName::from_bytes(bytes).is_err(), "{bytes:02x?}");
    }
}
