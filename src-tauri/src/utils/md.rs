pub fn generate_toc(md: &str) -> String {
    let mut toc = String::from("## Indice\n\n");
    let mut in_code_block = false;

    for line in md.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if !in_code_block && line.starts_with('#') {
            let level = line.chars().take_while(|c| *c == '#').count();
            if level > 0 && level <= 6 {
                let rest = &line[level..].trim();
                if !rest.is_empty() {
                    let ident = "  ".repeat(level - 1);
                    let slug = rest
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_alphanumeric() || *c == ' ')
                        .collect::<String>()
                        .replace(" ", "-");
                    toc.push_str(&format!("{}- [{}](#{})\n", ident, rest, slug));
                }
            }
        }
    }
    toc
}
