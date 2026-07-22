//! Phase 1 reproduction attempt for the production abort:
//! "conflict: contextual chunk span does not match its source body"
//! (diagnostic-dee83e37, worker job 019f7fa7, LME-V2 docs ingestion at 139/670).
//!
//! The compiler invariant (lib.rs mint_compiled_citations) requires, for every
//! contextual chunk with source_span "S-E": source_body[S..E] == chunk.body.
//! Both resource and episode chunkers mint chunk.body = body[S..E], so the
//! invariant holds *within one compile*. This test hammers the resource chunker
//! with the adversarial byte shapes the LME-V2 docs corpus contains (CRLF,
//! interior blank lines, multi-byte UTF-8, U+2028/U+2029 separators, fenced code,
//! edge whitespace) and asserts the invariant DIRECTLY, to find any input where
//! the minted span and body diverge.

use memphant_core::service::resource_chunks_for_test;

fn assert_invariant(label: &str, body: &str) {
    let chunks = resource_chunks_for_test(body);
    for c in &chunks {
        let span = c.source_span.as_deref().expect("chunk must carry a span");
        let (s, e) = span.split_once('-').expect("span is S-E");
        let (s, e): (usize, usize) = (s.parse().unwrap(), e.parse().unwrap());
        let slice = body.get(s..e);
        assert!(
            slice == Some(c.body.as_str()),
            "[{label}] span {s}..{e} slices {:?} but chunk.body is {:?}",
            slice,
            c.body
        );
    }
}

#[test]
fn resource_chunk_span_matches_body_on_adversarial_inputs() {
    let para = |n: usize| {
        format!(
            "Paragraph {n} carries enough words to matter for the char-budget windower so that the segmenter is forced to open more than a single window across the whole document body here."
        )
    };
    let big = |sep: &str, mid: &str| {
        format!(
            "{}{sep}{mid}{sep}{}{sep}{mid}{sep}{}",
            para(1),
            para(2),
            para(3)
        )
    };
    assert_invariant("lf-blank", &big("\n", "\n"));
    assert_invariant("crlf-blank", &big("\r\n", "\r\n"));
    assert_invariant("crlf-mixed", &big("\r\n", "\n"));
    assert_invariant("trailing-ws", &format!("{}   \n\n{}   ", para(1), para(2)));
    assert_invariant("leading-ws", &format!("   {}\n\n   {}", para(1), para(2)));
    // multi-byte + unicode separators the corpus is known to contain.
    assert_invariant("multibyte", &big("\n", "café ☕ 日本語 —— \u{2028}"));
    assert_invariant("u2028", &format!("{}\u{2028}\u{2028}{}", para(1), para(2)));
    assert_invariant("u2029", &format!("{}\u{2029}{}", para(1), para(2)));
    assert_invariant(
        "fenced",
        &format!(
            "{}\n\n```\ncode\n\nblank in fence\n```\n\n{}",
            para(1),
            para(2)
        ),
    );
    // very long single paragraph -> split_oversized path over multi-byte text
    let long_mb = "日本語テキスト".repeat(2000);
    assert_invariant("oversized-mb", &long_mb);
}
