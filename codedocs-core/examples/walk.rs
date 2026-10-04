//! Walk a folder and print the result as JSON, for browser-side verification.
//!
//! The browser checks need the *real* walker's output. Reimplementing the walk in
//! JavaScript would test the copy, not the shipped logic, which is how the
//! original bug stayed invisible.
//!
//! ```sh
//! cargo run -p codedocs-core --example walk -- /path/to/folder
//! ```

use std::path::Path;

use codedocs_core::{build_tree, WalkLimits};

fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        eprintln!("usage: walk <folder>");
        std::process::exit(2);
    };

    match build_tree(Path::new(&arg), WalkLimits::interactive()) {
        Ok(tree) => {
            // Hand-rolled JSON: the shapes are simple and this avoids pulling a
            // serialisation dependency into the crate for a dev tool.
            let entries: Vec<String> = tree.entries.iter().map(entry_json).collect();
            let truncated = match tree.truncated {
                Some(t) => format!(
                    "\"{}\"",
                    match t {
                        codedocs_core::Truncation::Depth { .. } => "depth",
                        codedocs_core::Truncation::Entries { .. } => "entries",
                    }
                ),
                None => "null".to_string(),
            };
            println!(
                "{{\"entries\":[{}],\"truncated\":{},\"fileCount\":{}}}",
                entries.join(","),
                truncated,
                tree.file_count()
            );
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn entry_json(entry: &codedocs_core::FileEntry) -> String {
    let name = json_string(&entry.name);
    let path = json_string(&entry.path);
    if entry.is_dir {
        let children: Vec<String> = entry.children.iter().map(entry_json).collect();
        format!(
            "{{\"name\":{name},\"path\":{path},\"is_dir\":true,\"children\":[{}]}}",
            children.join(",")
        )
    } else {
        format!("{{\"name\":{name},\"path\":{path},\"is_dir\":false,\"children\":[]}}")
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
