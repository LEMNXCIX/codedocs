//! Print the live-format spans of a markdown file as JSON.
//!
//! The browser checks need the *real* span computation's output, in UTF-16
//! code units as CodeMirror counts them. Reimplementing it in JavaScript
//! would test the copy, not the shipped logic.
//!
//! ```sh
//! cargo run -p codedocs-core --example live -- /path/to/file.md
//! ```

use codedocs_core::live_spans;

fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        eprintln!("usage: live <file>");
        std::process::exit(2);
    };
    let content = match std::fs::read_to_string(&arg) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    // Hand-rolled JSON: the shapes are simple and this avoids pulling a
    // serialisation dependency into the crate for a dev tool.
    let live = live_spans(&content);
    let ranges: Vec<String> = live.ranges.iter().map(|r| r.to_string()).collect();
    let tags: Vec<String> = live.tags.iter().map(|t| (*t as u32).to_string()).collect();
    println!(
        "{{\"ranges\":[{}],\"tags\":[{}]}}",
        ranges.join(","),
        tags.join(",")
    );
}
