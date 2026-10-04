//! Render markdown from stdin to stdout using the real renderer.
//!
//! Exists so the output of `codedocs_core` can be fed to a browser and checked
//! against a real HTML parser and a real CSP engine, rather than only being
//! asserted against string patterns in unit tests.
//!
//! ```sh
//! printf '# Title\n\n$x$\n' | cargo run -p codedocs-core --example render
//! ```

use std::io::Read;

fn main() {
    let mut markdown = String::new();
    std::io::stdin()
        .read_to_string(&mut markdown)
        .expect("failed to read markdown from stdin");

    print!("{}", codedocs_core::render_markdown(&markdown));
}
