use pulldown_cmark::{html, Options, Parser};

fn get_options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options
}

fn preprocess_math(content: &str) -> String {
    let mut result = content.to_string();

    result = result.replace("[!NOTE]", "**NOTE**");
    result = result.replace("[!TIP]", "**TIP**");
    result = result.replace("[!IMPORTANT]", "**IMPORTANT**");
    result = result.replace("[!WARNING]", "**WARNING**");
    result = result.replace("[!CAUTION]", "**CAUTION**");

    let display_re = regex_lite::Regex::new(r"\$\$([\s\S]*?)\$\$").unwrap();
    result = display_re
        .replace_all(&result, r#"<div class="math-display">$1</div>"#)
        .to_string();

    let inline_re = regex_lite::Regex::new(r"\$([^\$\n]+?)\$").unwrap();
    result = inline_re
        .replace_all(&result, r#"<span class="math-inline">$1</span>"#)
        .to_string();

    result
}

fn preprocess_mermaid(content: &str) -> String {
    let re = regex_lite::Regex::new(r"```mermaid\s*\n([\s\S]*?)\n```").unwrap();
    re.replace_all(content, |caps: &regex_lite::Captures| {
        let code = &caps[1];
        let escaped = code.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
        format!(r#"<pre class="mermaid-block" data-mermaid="{}">{}</pre>"#, escaped, escaped)
    })
    .to_string()
}

pub fn render_markdown(content: &str) -> String {
    let preprocessed = preprocess_mermaid(&preprocess_math(content));

    let options = get_options();
    let parser = Parser::new_ext(&preprocessed, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    html_output
}

pub fn extract_headings(content: &str) -> Vec<Heading> {
    let options = get_options();
    let parser = Parser::new_ext(content, options);

    let mut headings = Vec::new();
    for event in parser {
        match event {
            pulldown_cmark::Event::Start(pulldown_cmark::Tag::Heading { level, .. }) => {
                headings.push(Heading {
                    level: level as u8,
                    text: String::new(),
                });
            }
            pulldown_cmark::Event::Text(text) => {
                if let Some(last) = headings.last_mut() {
                    if last.text.is_empty() {
                        last.text = text.to_string();
                    }
                }
            }
            pulldown_cmark::Event::End(pulldown_cmark::TagEnd::Heading(_)) => {}
            _ => {}
        }
    }

    headings
}

#[derive(Debug, Clone)]
pub struct Heading {
    pub level: u8,
    pub text: String,
}
