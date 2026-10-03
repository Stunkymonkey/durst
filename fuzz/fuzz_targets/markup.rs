//! Notification bodies come from any app on the session bus: the markup
//! parser must not panic or hang on any input, and keeps its invariants.

#![no_main]

use libfuzzer_sys::fuzz_target;

#[path = "../../durst/src/ui/markup.rs"]
#[allow(dead_code)]
mod markup;

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

fuzz_target!(|input: &str| {
    let runs = markup::parse(input);
    for (i, run) in runs.iter().enumerate() {
        assert!(!run.text.is_empty(), "empty run");
        if let Some(next) = runs.get(i + 1) {
            let format = |r: &markup::Run| (r.bold, r.italic, r.underline, r.link.clone());
            assert_ne!(
                format(run),
                format(next),
                "runs of the same format not merged"
            );
        }
    }
    let _ = markup::first_link(&runs);

    // escaped text comes back unchanged and unformatted
    let plain = markup::parse(&escape(input));
    let text: String = plain.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(text, input);
    assert!(plain.len() <= 1, "plain text split into runs");
});
