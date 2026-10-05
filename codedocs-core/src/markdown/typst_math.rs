//! KaTeX-flavoured math → Typst math.
//!
//! # Why this file exists at all
//!
//! The obvious design — hand the TeX straight to Typst, since "the syntaxes
//! coincide in the usual cases" — does not work, and it fails in the worst
//! possible way. Typst 0.15.1 has **no** TeX-command syntax: `\frac`, `\sum`
//! and `\alpha` are all read as the backslash operator followed by an
//! identifier, so `$\frac{a}{b}$` reports "unknown variable: rac" and
//! `$\sum_i$` reports "unknown variable: um". A note with one `$$…$$` block in
//! it does not render badly, it produces **no PDF at all**.
//!
//! Every spelling below was measured against the real compiler, one expression
//! per file. That last detail matters: Typst reports *parse* errors before it
//! evaluates anything, so a single unparseable line hides every later error. An
//! earlier batch that compiled all the candidates into one file reported one
//! error and looked like twenty successes.
//!
//! # The shape of the translation
//!
//! Typst's own convention is that a TeX command name minus its backslash is
//! usually already a valid Typst identifier — `\alpha` is `alpha`,
//! `\nabla` is `nabla`, `\sin` is `sin`. So the default rule is "drop the
//! backslash" and [`EXCEPTIONS`] lists the ones that need real work.
//!
//! The second half is [`space_bare_identifiers`]: Typst reads a run of letters
//! as one identifier, so KaTeX's `$mc$` (two italic letters) becomes Typst's
//! `mc` (an unknown variable called `mc`) and fails the compilation. Splitting
//! the run gives Typst what it wants and KaTeX's meaning along with it.
//!
//! Nothing here can produce Typst *code* from document text: the output is an
//! expression, and every character that ends it (`$`) was already consumed by
//! the math splitter before this module sees anything.

/// Convert one KaTeX expression into a Typst expression.
///
/// Display math that spans rows (`\begin{align}`) is split by
/// [`tex_to_typst_blocks`] instead; this returns the whole expression as one
/// block.
pub fn tex_to_typst(tex: &str) -> String {
    translate(&space_bare_identifiers(&normalize_rows(tex)))
}

/// Turn KaTeX's `\\` row separator into a newline, before anything else looks
/// at the expression.
///
/// It has to happen here: the markdown parser has already eaten one of the two
/// backslashes (`\\` in markdown source is an escaped backslash), so by the
/// time this module sees a row separator it is a *single* backslash. Reading
/// rows as `split("\\\\")` therefore finds nothing and `\begin{cases} a \\ b
/// \end{cases}` silently becomes one row.
///
/// A backslash followed by whitespace is the separator; a backslash followed by
/// letters is a command and is left alone.
fn normalize_rows(tex: &str) -> String {
    let chars: Vec<char> = tex.chars().collect();
    let mut out = String::with_capacity(tex.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\\' {
            let next = chars.get(index + 1).copied();
            match next {
                // `\\` that survived: the separator is both characters.
                Some('\\') => {
                    out.push('\n');
                    index += 2;
                    continue;
                }
                Some(ch) if ch.is_whitespace() => {
                    out.push('\n');
                    index += 2;
                    continue;
                }
                // A command, or a trailing lone backslash: copy it and let the
                // translator deal with it.
                _ => {
                    out.push('\\');
                    index += 1;
                    continue;
                }
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

/// Convert display math into the expressions to typeset, one per row.
///
/// Only environments that mean "several rows" produce more than one entry: an
/// `align` in KaTeX is a multi-line display, and Typst has no `align` function
/// at all, so each row becomes its own centred paragraph. That loses the
/// column alignment of an `&`-separated `align`, which is the honest trade for
/// being able to render it: the alternative is a compile error.
pub fn tex_to_typst_blocks(tex: &str) -> Vec<String> {
    let normalized = normalize_rows(tex);
    let trimmed = normalized.trim();
    for environment in ["align", "aligned", "gathered", "split", "eqnarray"] {
        let open = format!("\\begin{{{environment}}}");
        let close = format!("\\end{{{environment}}}");
        if let Some(rest) = trimmed.strip_prefix(&open) {
            let body = rest.strip_suffix(&close).unwrap_or(rest);
            let rows: Vec<String> = body
                .split('\n')
                .map(|row| tex_to_typst(row).trim().to_string())
                .filter(|row| !row.is_empty())
                .collect();
            if rows.is_empty() {
                return vec![String::new()];
            }
            return rows;
        }
    }
    vec![tex_to_typst(trimmed)]
}

/// What a backslash command becomes.
enum Shape {
    /// A fixed replacement, e.g. `\le` → `<=`.
    Fixed(&'static str),
    /// A function call with one argument, e.g. `\hat{x}` → `hat(x)`.
    One(&'static str),
    /// A function call with two arguments, e.g. `\frac{a}{b}` → `frac(a, b)`.
    Two(&'static str),
    /// A call whose argument is *text*, e.g. `\text{si}` → `upright("si")`.
    Text(&'static str),
    /// The square root, with its optional index.
    Root,
    /// `\left(` … `\right)`, and the delimiter pairs that go with it.
    Left,
    /// `\begin{cases}` and friends.
    Environment,
    /// Drop the backslash and keep the name: `\alpha` → `alpha`.
    Name,
}

/// The commands that do not survive a bare "drop the backslash".
///
/// Kept deliberately short: a name that is absent falls through to
/// [`Shape::Name`] and works if Typst knows the identifier, so only the
/// commands that are *wrong* or *missing* need an entry.
fn lookup(command: &str) -> Shape {
    match command {
        // Relations: Typst spells these as operators.
        "le" | "leq" => Shape::Fixed("<="),
        "ge" | "geq" => Shape::Fixed(">="),
        "ne" | "neq" => Shape::Fixed("!="),

        // Symbols whose Typst name differs.
        "cdot" => Shape::Fixed("dot"),
        "infty" => Shape::Fixed("infinity"),
        "int" => Shape::Fixed("integral"),
        "iint" => Shape::Fixed("integral.double"),
        "prod" => Shape::Fixed("product"),
        "ldots" | "dots" | "cdots" => Shape::Fixed("dots"),
        "to" | "rightarrow" => Shape::Fixed("arrow.r"),
        "leftarrow" => Shape::Fixed("arrow.l"),
        "Leftarrow" => Shape::Fixed("arrow.l.r"),
        "Rightarrow" => Shape::Fixed("arrow.r.r"),
        "leftrightarrow" => Shape::Fixed("arrow.l.r"),
        "pm" => Shape::Fixed("plus.minus"),
        "mp" => Shape::Fixed("minus.plus"),
        "qquad" => Shape::Fixed("quad quad"),
        "thinspace" | "medspace" | "thickspace" => Shape::Fixed("quad"),
        // Spacing commands are punctuation in Typst math.
        "," => Shape::Fixed(","),
        ":" => Shape::Fixed(" "),
        ";" => Shape::Fixed(";"),
        "!" => Shape::Fixed("!"),
        " " => Shape::Fixed(" "),
        "%" => Shape::Fixed("%"),
        "_" => Shape::Fixed("_"),
        "&" => Shape::Fixed("&"),
        "\\{" => Shape::Fixed("brace.l"),
        "\\}" => Shape::Fixed("brace.r"),
        "{" => Shape::Fixed("brace.l"),
        "}" => Shape::Fixed("brace.r"),

        // Functions that take their arguments in braces.
        "frac" | "dfrac" | "tfrac" | "over" => Shape::Two("frac"),
        "binom" | "dbinom" | "tbinom" => Shape::Two("binom"),
        "underbrace" | "overbrace" => Shape::Two("underbrace"),
        "sqrt" => Shape::Root,

        // Font and text commands.
        "text" | "textrm" | "textbf" | "textit" | "textnormal" | "mbox" | "mathrm" => {
            Shape::Text("upright")
        }
        "operatorname" => Shape::Text("op"),
        "mathbf" | "bm" => Shape::One("bold"),
        "mathit" => Shape::One("italic"),
        "mathbb" => Shape::One("bb"),
        "mathcal" => Shape::One("cal"),
        "mathsf" => Shape::One("sans"),
        "mathtt" => Shape::One("mono"),

        // Accents and decorations.
        "hat" | "widehat" => Shape::One("hat"),
        "bar" => Shape::One("bar"),
        "vec" => Shape::One("vec"),
        "tilde" | "widetilde" => Shape::One("tilde"),
        "dot" | "dotaccent" => Shape::One("dot"),
        "ddot" => Shape::One("dot.double"),
        "overline" => Shape::One("overline"),
        "underline" => Shape::One("underline"),

        "left" => Shape::Left,
        "begin" => Shape::Environment,

        _ => Shape::Name,
    }
}

/// Commands whose braces hold text rather than an expression.
///
/// Only consulted by [`space_bare_identifiers`], which must leave the inside of
/// these alone: `\text{multiple words}` must not become `m u l t i p l e`, and
/// `\begin{cases}` must keep an environment name that the environment lookup
/// can still recognise.
const TEXT_ARGUMENTS: &[&str] = &[
    "text",
    "textrm",
    "textbf",
    "textit",
    "textnormal",
    "mbox",
    "mathrm",
    "operatorname",
    "begin",
];

/// Put a space between the letters of every bare identifier.
///
/// Typst reads `mc` as one identifier and fails with "unknown variable: mc",
/// where KaTeX reads two italic letters. `m c` is how Typst spells the second
/// reading, and it also renders as KaTeX does.
///
/// Two things are left alone: a run of letters right after a backslash (those
/// are the command names, which the translator still needs intact), and the
/// inside of a text-like command's argument.
fn space_bare_identifiers(tex: &str) -> String {
    let chars: Vec<char> = tex.chars().collect();
    let mut out = String::with_capacity(tex.len());
    let mut index = 0;
    // Set when the previous character was a backslash, which makes the next run
    // a command name rather than an identifier.
    let mut after_backslash = false;

    while index < chars.len() {
        if chars[index] == '\\' {
            // Copy the command name verbatim, then a text argument verbatim.
            let start = index + 1;
            let mut end = start;
            while end < chars.len() && chars[end].is_ascii_alphabetic() {
                end += 1;
            }
            let name: String = chars[start..end].iter().collect();
            out.push('\\');
            out.push_str(&name);
            index = end;
            after_backslash = true;

            if TEXT_ARGUMENTS.contains(&name.as_str()) {
                index = copy_group_verbatim(&chars, index, &mut out);
            }
            continue;
        }

        if chars[index].is_ascii_alphabetic() && !after_backslash {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_alphabetic() {
                index += 1;
            }
            let run = chars[start..index].iter().collect::<String>();
            // Single letters are already identifiers; `e` in `e^{i\pi}` must not
            // become anything else.
            if run.len() > 1 {
                out.push_str(
                    &run.chars()
                        .map(|c| c.to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            } else {
                out.push_str(&run);
            }
            continue;
        }

        if !chars[index].is_ascii_alphabetic() {
            after_backslash = false;
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

/// Copy a `{…}` group verbatim, or nothing when there is no group here.
fn copy_group_verbatim(chars: &[char], mut index: usize, out: &mut String) -> usize {
    while index < chars.len() && chars[index].is_whitespace() {
        index += 1;
    }
    if index >= chars.len() || chars[index] != '{' {
        return index;
    }
    let mut depth = 0usize;
    while index < chars.len() {
        match chars[index] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                out.push('}');
                index += 1;
                if depth == 0 {
                    return index;
                }
                continue;
            }
            _ => {}
        }
        out.push(chars[index]);
        index += 1;
    }
    index
}

/// Translate the spaced TeX into a Typst expression.
fn translate(tex: &str) -> String {
    let chars: Vec<char> = tex.chars().collect();
    let mut out = String::with_capacity(tex.len());
    let mut index = 0;

    while index < chars.len() {
        match chars[index] {
            '\\' => index = command(&chars, index, &mut out),
            '^' | '_' => index = script(&chars, index, &mut out),
            // The two characters that would let document text *become* Typst
            // code inside a math span. A `#` starts a code expression — which
            // can read files — and a `"` opens a string literal; both come
            // straight from the document, so both are neutralised here.
            '#' => {
                out.push_str("\\#");
                index += 1;
            }
            '"' => {
                // Typst's escape for the character, as a one-character string:
                // `\#`-style escapes do not exist for a quote.
                out.push_str("\"\\u{22}\"");
                index += 1;
            }
            _ => {
                // The same separation rule as `push_token`, for a character
                // copied straight through: after a translated symbol like
                // `brace.l`, a digit must not glue itself to the identifier.
                if needs_space(out.as_str(), chars[index]) {
                    out.push(' ');
                }
                out.push(chars[index]);
                index += 1;
            }
        }
    }
    out
}

/// One backslash command, starting at `index`.
fn command(chars: &[char], index: usize, out: &mut String) -> usize {
    let start = index + 1;
    let mut end = start;
    while end < chars.len() && chars[end].is_ascii_alphabetic() {
        end += 1;
    }

    // `\\` is a row separator, `\,` and friends are single-character commands,
    // and `\ ` is an escaped space.
    if end == start {
        // `\{` and `\}` are two characters, so they are looked up as a pair
        // rather than as the backslash alone.
        if let Some(pair) = chars.get(start..start + 2) {
            let name: String = pair.iter().collect();
            if let Shape::Fixed(replacement) = lookup(&name) {
                push_token(out, replacement);
                return start + 2;
            }
        }
        let symbol = chars.get(start).copied().unwrap_or('\\');
        match lookup(&symbol.to_string()) {
            Shape::Fixed(replacement) => push_token(out, replacement),
            _ => {
                out.push('\\');
                out.push(symbol);
            }
        }
        return start + 1;
    }

    let name: String = chars[start..end].iter().collect();
    let shape = lookup(&name);
    match shape {
        Shape::Fixed(replacement) => {
            push_token(out, replacement);
            end
        }
        Shape::Name => {
            // The default: Typst's identifier is the command without its
            // backslash.
            push_token(out, &name);
            end
        }
        Shape::One(function) => {
            let mut cursor = end;
            let argument = argument(chars, &mut cursor).unwrap_or_default();
            push_call(out, function, &[argument]);
            cursor
        }
        Shape::Two(function) => {
            let mut cursor = end;
            let first = argument(chars, &mut cursor).unwrap_or_default();
            let second = argument(chars, &mut cursor).unwrap_or_default();
            push_call(out, function, &[first, second]);
            cursor
        }
        Shape::Text(function) => {
            let mut cursor = end;
            let raw = raw_argument(chars, &mut cursor).unwrap_or_default();
            push_call(out, function, &[format!("\"{}\"", escape_string(&raw))]);
            cursor
        }
        Shape::Root => {
            let mut cursor = end;
            // `\sqrt[3]{x}` carries an index, `\sqrt{x}` does not.
            let mut index_argument = None;
            skip_spaces(chars, &mut cursor);
            if chars.get(cursor) == Some(&'[') {
                let mut close = cursor + 1;
                while close < chars.len() && chars[close] != ']' {
                    close += 1;
                }
                let raw: String = chars[cursor + 1..close].iter().collect();
                index_argument = Some(translate(&raw));
                cursor = (close + 1).min(chars.len());
            }
            let radicand = argument(chars, &mut cursor).unwrap_or_default();
            match index_argument {
                Some(index_argument) => push_call(out, "root", &[index_argument, radicand]),
                None => push_call(out, "sqrt", &[radicand]),
            }
            cursor
        }
        Shape::Left => left_right(chars, end, out),
        Shape::Environment => environment(chars, end, out),
    }
}

/// `^` and `_`, with the group that follows them.
///
/// Typst needs the parentheses when the argument is more than one character:
/// `x^2` is fine, `x^{i\pi}` is not (`x^(i pi)` is).
fn script(chars: &[char], index: usize, out: &mut String) -> usize {
    let marker = chars[index];
    let mut cursor = index + 1;
    let braced = chars.get(cursor) == Some(&'{');
    let argument = argument(chars, &mut cursor).unwrap_or_default();
    out.push(marker);
    let argument = argument.trim();
    if braced && argument.chars().count() > 1 {
        out.push('(');
        out.push_str(argument);
        out.push(')');
    } else {
        out.push_str(argument);
    }
    cursor
}

/// The next argument, as translated Typst: a `{…}` group or a single token.
fn argument(chars: &[char], cursor: &mut usize) -> Option<String> {
    skip_spaces(chars, cursor);
    match chars.get(*cursor) {
        Some('{') => {
            let (raw, next) = group(chars, *cursor);
            *cursor = next;
            Some(translate(&raw))
        }
        Some('\\') => {
            let mut out = String::new();
            let next = command(chars, *cursor, &mut out);
            *cursor = next;
            Some(out)
        }
        Some(_) => {
            let ch = chars[*cursor];
            *cursor += 1;
            Some(ch.to_string())
        }
        None => None,
    }
}

/// The next argument *untranslated*, for text that becomes a string.
fn raw_argument(chars: &[char], cursor: &mut usize) -> Option<String> {
    skip_spaces(chars, cursor);
    match chars.get(*cursor) {
        Some('{') => {
            let (raw, next) = group(chars, *cursor);
            *cursor = next;
            Some(raw)
        }
        Some(_) => {
            let ch = chars[*cursor];
            *cursor += 1;
            Some(ch.to_string())
        }
        None => None,
    }
}

/// The text inside a `{…}` group, plus the index just past its `}`.
fn group(chars: &[char], start: usize) -> (String, usize) {
    let mut inner = String::new();
    let mut depth = 0usize;
    let mut index = start;
    while index < chars.len() {
        match chars[index] {
            '{' => {
                depth += 1;
                if depth > 1 {
                    inner.push('{');
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (inner, index + 1);
                }
                inner.push('}');
            }
            other => inner.push(other),
        }
        index += 1;
    }
    (inner, index)
}

fn skip_spaces(chars: &[char], cursor: &mut usize) {
    while chars.get(*cursor).is_some_and(|c| c.is_whitespace()) {
        *cursor += 1;
    }
}

/// Whether `next` must be separated from what `out` already ends with.
///
/// Two identifiers or an identifier and a digit run together into one name,
/// which is how `mc` becomes an unknown variable and how `brace.l1` becomes
/// another one. Two *digits*, on the other hand, are a single number: `log_(10)`
/// must not come out as `log_(1 0)`.
fn needs_space(out: &str, next: char) -> bool {
    if !next.is_alphanumeric() {
        return false;
    }
    match out.chars().next_back() {
        Some(previous) if previous.is_alphanumeric() => {
            previous.is_alphabetic() || next.is_alphabetic()
        }
        _ => false,
    }
}

/// Push a translated token, keeping it a *separate* identifier.
///
/// `space_bare_identifiers` runs before this, on the TeX, where the structure
/// still says whether two letters were adjacent in the source. Translation can
/// bring them together again — `i\pi` becomes `i` followed by the symbol `pi`,
/// and concatenating those gives `ipi`, which Typst reads as one unknown
/// variable. So every emitted name is checked against what came before.
fn push_token(out: &mut String, token: &str) {
    if let Some(first) = token.chars().next() {
        if needs_space(out, first) {
            out.push(' ');
        }
    }
    out.push_str(token);
}

/// Push `name(arg, …)`, the same separation rule as [`push_token`].
///
/// Without it `x\frac{a}{b}` would come out as `xfrac(a, b)`: one identifier
/// instead of a variable followed by a call.
fn push_call(out: &mut String, name: &str, arguments: &[String]) {
    push_token(out, name);
    out.push('(');
    out.push_str(&arguments.join(", "));
    out.push(')');
}

/// Escape a value for a Typst string literal.
///
/// The math body is document text, and it reaches a string literal only through
/// `\text{…}`. Both characters that can end the literal are escaped, which makes
/// the escape total — see the same reasoning in `typst.rs`, where the same rule
/// is what keeps a URL from closing its own literal.
fn escape_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// `\left( … \right)` and the delimiters that go with it.
///
/// Typst has no `\left`: the sizing delimiter is `lr`, and the fixed ones are
/// their own functions (`abs`, `norm`, `brace`, `bracket`).
fn left_right(chars: &[char], index: usize, out: &mut String) -> usize {
    let (open, after_open) = delimiter(chars, index);
    let mut cursor = after_open;

    // Everything up to the matching `\right`, translated.
    let start = cursor;
    let mut depth = 0usize;
    while cursor < chars.len() {
        if chars[cursor] == '\\' {
            let mut lookahead = cursor + 1;
            let mut name = String::new();
            while chars
                .get(lookahead)
                .is_some_and(|c| c.is_ascii_alphabetic())
            {
                name.push(chars[lookahead]);
                lookahead += 1;
            }
            if name == "left" || name == "right" {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            } else if name == "begin" || name == "end" {
                depth += 1;
            }
        }
        cursor += 1;
    }
    let inner = chars[start..cursor].iter().collect::<String>();
    // Trimmed: the spaces around `\left( … \right)` are TeX's, and Typst would
    // count them as content of the delimiter.
    let body = translate(&inner).trim().to_string();

    // Past the `\right` and its delimiter.
    let mut after = cursor;
    if chars.get(after) == Some(&'\\') {
        let mut lookahead = after + 1;
        let mut name = String::new();
        while chars
            .get(lookahead)
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            name.push(chars[lookahead]);
            lookahead += 1;
        }
        if name == "right" {
            after = lookahead;
            let (_, next) = delimiter(chars, after);
            after = next;
        }
    }

    match open.as_str() {
        "(" => push_call(out, "lr", std::slice::from_ref(&body)),
        "|" => push_call(out, "abs", std::slice::from_ref(&body)),
        "\\|" => push_call(out, "norm", std::slice::from_ref(&body)),
        "\\{" => push_call(out, "brace", std::slice::from_ref(&body)),
        "[" => push_call(out, "bracket", std::slice::from_ref(&body)),
        // Any other pairing: the delimiters are dropped and the body stands on
        // its own, which is better than emitting something uncompilable.
        _ => out.push_str(&body),
    }
    after
}

/// The delimiter after `\left` or `\right`, and the index past it.
fn delimiter(chars: &[char], index: usize) -> (String, usize) {
    let mut cursor = index;
    skip_spaces(chars, &mut cursor);
    if chars.get(cursor) == Some(&'\\') {
        let mut end = cursor + 1;
        while chars.get(end).is_some_and(|c| c.is_ascii_alphabetic()) {
            end += 1;
        }
        if end > cursor + 1 {
            return (chars[cursor..end].iter().collect(), end);
        }
        // `\{`, `\}`, `\|`, `\langle`: the delimiter is a backslash and one more
        // character, and the two together are what decides the Typst function.
        let end = (cursor + 2).min(chars.len());
        return (chars[cursor..end].iter().collect(), end);
    }
    match chars.get(cursor) {
        Some(ch) => (ch.to_string(), cursor + 1),
        None => (String::new(), cursor),
    }
}

/// `\begin{…}` for the environments Typst can render.
fn environment(chars: &[char], index: usize, out: &mut String) -> usize {
    let mut cursor = index;
    skip_spaces(chars, &mut cursor);
    if chars.get(cursor) != Some(&'{') {
        return cursor;
    }
    let (name, after_name) = group(chars, cursor);
    cursor = after_name;

    // The body runs to `\end{…}`.
    let start = cursor;
    let mut depth = 0usize;
    while cursor < chars.len() {
        if chars[cursor] == '\\' {
            let mut lookahead = cursor + 1;
            let mut word = String::new();
            while chars
                .get(lookahead)
                .is_some_and(|c| c.is_ascii_alphabetic())
            {
                word.push(chars[lookahead]);
                lookahead += 1;
            }
            if word == "begin" {
                depth += 1;
            } else if word == "end" {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
        }
        cursor += 1;
    }
    let inner = chars[start..cursor].iter().collect::<String>();
    // Past `\end{…}`.
    let mut after = cursor;
    if chars.get(after) == Some(&'\\') {
        let mut lookahead = after + 1;
        let mut word = String::new();
        while chars
            .get(lookahead)
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            word.push(chars[lookahead]);
            lookahead += 1;
        }
        if word == "end" {
            let mut end_cursor = lookahead;
            skip_spaces(chars, &mut end_cursor);
            if chars.get(end_cursor) == Some(&'{') {
                let (_, next) = group(chars, end_cursor);
                after = next;
            } else {
                after = lookahead;
            }
        }
    }

    // Rows are separated by the newline `normalize_rows` left behind, and
    // columns by `&`, which is exactly the shape Typst's `cases` and `mat` take.
    let rows: Vec<String> = inner
        .split('\n')
        .map(|row| translate(row).trim().to_string())
        .filter(|row| !row.is_empty())
        .collect();
    let body = rows.join(", ");

    match name.as_str() {
        "cases" | "dcases" | "rcases" => push_call(out, "cases", &[body]),
        "pmatrix" | "bmatrix" | "Bmatrix" | "vmatrix" | "Vmatrix" | "matrix" | "array"
        | "smallmatrix" => push_call(out, "mat", &[body]),
        // Typst has no `align`: `tex_to_typst_blocks` splits the rows before
        // ever getting here. Reached only for a single-row align.
        "align" | "aligned" | "gathered" | "split" | "eqnarray" => out.push_str(&body),
        // An environment this module does not model keeps its text, so the
        // document still exports rather than failing outright.
        _ => out.push_str(&body),
    }
    after
}

#[cfg(test)]
mod tests {
    use super::*;

    fn math(tex: &str) -> String {
        tex_to_typst(tex)
    }

    // ---- Symbols ----

    #[test]
    fn a_greek_command_loses_only_its_backslash() {
        // Typst's identifier is the command without the backslash, which is
        // what makes the default rule viable at all.
        assert_eq!(math("\\alpha"), "alpha");
        assert_eq!(math("\\pi"), "pi");
        assert_eq!(math("\\theta"), "theta");
        assert_eq!(math("\\nabla"), "nabla");
    }

    #[test]
    fn relations_become_operators() {
        // Typst has no `\le`; it has `<=`.
        assert_eq!(math("a \\le b"), "a <= b");
        assert_eq!(math("a \\geq b"), "a >= b");
        assert_eq!(math("a \\ne b"), "a != b");
    }

    #[test]
    fn symbols_whose_typst_name_differs() {
        assert_eq!(math("\\cdot"), "dot");
        assert_eq!(math("\\infty"), "infinity");
        assert_eq!(math("\\int"), "integral");
        assert_eq!(math("\\ldots"), "dots");
        assert_eq!(math("\\rightarrow"), "arrow.r");
        assert_eq!(math("\\pm"), "plus.minus");
        assert_eq!(math("\\qquad"), "quad quad");
    }

    #[test]
    fn an_unknown_command_keeps_its_name() {
        // Not mapped, and Typst will say so out loud rather than this module
        // silently swallowing the expression.
        assert_eq!(math("\\nosuchthing"), "nosuchthing");
    }

    // ---- Functions with arguments ----

    #[test]
    fn a_fraction_becomes_a_call() {
        // The case that makes this module necessary: Typst has no `\frac` at
        // all and reports "unknown variable: rac" for it.
        assert_eq!(math("\\frac{a}{b}"), "frac(a, b)");
        assert_eq!(math("\\dfrac{1}{2}"), "frac(1, 2)");
    }

    #[test]
    fn a_root_is_a_call_and_keeps_its_index() {
        assert_eq!(math("\\sqrt{x}"), "sqrt(x)");
        assert_eq!(math("\\sqrt[3]{x}"), "root(3, x)");
    }

    #[test]
    fn an_environment_becomes_a_call() {
        assert_eq!(
            math("\\begin{cases} 1 & x>0 \\\\ 0 & \\text{no} \\end{cases}"),
            "cases(1 & x>0, 0 & upright(\"no\"))"
        );
        assert_eq!(
            math("\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}"),
            "mat(1 & 2, 3 & 4)"
        );
    }

    #[test]
    fn left_and_right_become_a_sizing_delimiter() {
        assert_eq!(math("\\left( \\frac{a}{b} \\right)"), "lr(frac(a, b))");
        assert_eq!(math("\\left| x \\right|"), "abs(x)");
        assert_eq!(math("\\left\\{ x \\right\\}"), "brace(x)");
    }

    #[test]
    fn a_bare_brace_becomes_the_typst_bracket_symbol() {
        // `\left\{` is the *sizing* brace; a bare `\{` is just the character.
        assert_eq!(math("\\{1, 2\\}"), "brace.l 1, 2 brace.r");
    }

    #[test]
    fn a_hash_in_math_cannot_start_a_code_expression() {
        // The same argument as the prose escaping, inside math: Typst reads `#`
        // as a code expression even between `$` delimiters, and a code
        // expression can read files.
        assert_eq!(math("a # b"), "a \\# b");
    }

    #[test]
    fn a_quote_in_math_cannot_open_a_string() {
        // Unescaped, the quote opens a literal that swallows the rest of the
        // document ("unclosed delimiter", no PDF). `\u{22}` is the character.
        assert_eq!(math("a \" b"), "a \"\\u{22}\" b");
    }

    // ---- Scripts ----

    #[test]
    fn a_braced_script_gets_parentheses() {
        // Typst needs them: `x^{i pi}` is not a script, it is `x` followed by
        // `^` and a stray identifier.
        assert_eq!(math("x^{i\\pi}"), "x^(i pi)");
        assert_eq!(math("a^{n}"), "a^n");
        assert_eq!(math("x^2"), "x^2");
        assert_eq!(math("x_{i=0}"), "x_(i=0)");
    }

    // ---- Text ----

    #[test]
    fn text_commands_become_a_string() {
        assert_eq!(math("\\text{si no}"), "upright(\"si no\")");
        assert_eq!(math("\\operatorname{tr}"), "op(\"tr\")");
        assert_eq!(math("\\mathrm{mc}"), "upright(\"mc\")");
    }

    #[test]
    fn a_text_argument_cannot_close_its_own_string() {
        // Same invariant as a URL in a link: both characters that end a Typst
        // string literal are escaped, so a quote in the document cannot end it
        // and turn the rest of the expression into code.
        assert_eq!(math("\\text{a\"b}"), "upright(\"a\\\"b\")");
        // The form that actually arrives: the markdown parser turns `\\` into
        // `\`, so a backslash inside `\text` is a single one — and it has to
        // come out escaped or the literal would end at it.
        assert_eq!(math("\\text{a\\b}"), "upright(\"a\\\\b\")");
    }

    #[test]
    fn font_commands_become_their_typst_names() {
        assert_eq!(math("\\mathbf{v}"), "bold(v)");
        assert_eq!(math("\\mathbb{R}"), "bb(R)");
        assert_eq!(math("\\mathcal{L}"), "cal(L)");
        assert_eq!(math("\\hat{x}"), "hat(x)");
        assert_eq!(math("\\overline{AB}"), "overline(A B)");
    }

    // ---- Bare identifiers ----

    #[test]
    fn a_bare_multi_letter_identifier_is_split() {
        // `E = mc^2` is the most famous formula there is, and Typst reads `mc`
        // as one unknown variable, which fails the whole compilation.
        assert_eq!(math("E = mc^2"), "E = m c^2");
    }

    #[test]
    fn command_names_are_never_split() {
        // Splitting these would break every command: `\sin` must stay `sin`.
        assert_eq!(math("\\sin x"), "sin x");
        assert_eq!(math("\\lim_{x \\to 0}"), "lim_(x arrow.r 0)");
        assert_eq!(math("\\log_{10} x"), "log_(10) x");
    }

    #[test]
    fn letters_inside_a_text_argument_are_never_split() {
        assert_eq!(
            math("\\text{multiple words}"),
            "upright(\"multiple words\")"
        );
    }

    #[test]
    fn single_letters_are_left_alone() {
        assert_eq!(math("e^{i\\pi}"), "e^(i pi)");
        assert_eq!(math("x_i^2"), "x_i^2");
    }

    #[test]
    fn a_word_inside_a_group_is_split() {
        // `x^{\mathrm{PDF}}` — a group argument is an expression, not text, so
        // its letters are identifiers.
        assert_eq!(math("\\frac{mc}{n}"), "frac(m c, n)");
    }

    // ---- Whole expressions ----

    #[test]
    fn a_row_separator_survives_the_markdown_parser() {
        // `\\` in a markdown source is an escaped backslash, so by the time the
        // expression arrives the separator is a single `\` before whitespace.
        // Reading rows as two backticks finds nothing, and a `cases` collapses
        // into a single row.
        assert_eq!(
            math("\\begin{cases} 1 & a \\ 0 & b \\end{cases}"),
            "cases(1 & a, 0 & b)"
        );
        assert_eq!(
            math("\\begin{pmatrix} 1 & 2 \\\\ 3 & 4 \\end{pmatrix}"),
            "mat(1 & 2, 3 & 4)"
        );
    }

    #[test]
    fn the_e_mc_squared_formula_becomes_valid_typst() {
        assert_eq!(math("E = mc^2"), "E = m c^2");
        assert_eq!(math("E = m c^2"), "E = m c^2");
    }

    #[test]
    fn a_sum_with_limits_becomes_valid_typst() {
        assert_eq!(
            math("\\sum_{i=0}^{n} x_i^2 = \\alpha"),
            "sum_(i=0)^n x_i^2 = alpha"
        );
    }

    #[test]
    fn an_unknown_environment_keeps_its_text() {
        // A document that exports with its content beats one that refuses to
        // export at all; the unmodelled environment's words still survive.
        assert_eq!(math("\\begin{diagram} a \\end{diagram}"), "a");
    }

    #[test]
    fn display_rows_are_split_into_separate_expressions() {
        // Typst has no `align` function, so each row becomes its own paragraph.
        assert_eq!(
            tex_to_typst_blocks("\\begin{align} a &= b \\\\ c &= d \\end{align}"),
            vec!["a &= b", "c &= d"]
        );
        assert_eq!(tex_to_typst_blocks("\\frac{a}{b}"), vec!["frac(a, b)"]);
        assert_eq!(
            tex_to_typst_blocks("\\begin{aligned} a &= b \\end{aligned}"),
            vec!["a &= b"]
        );
    }

    #[test]
    fn empty_and_blank_math_does_not_panic() {
        assert_eq!(math(""), "");
        assert_eq!(math("   "), "   ");
        assert_eq!(tex_to_typst_blocks(""), vec![String::new()]);
    }

    #[test]
    fn an_unbalanced_group_does_not_panic() {
        // Truncated input is normal here: the user is mid-keystroke.
        for tex in ["\\frac{a}", "\\sqrt", "{", "}", "\\left(", "\\begin{cases}"] {
            let _ = tex_to_typst(tex);
            let _ = tex_to_typst_blocks(tex);
        }
    }

    #[test]
    fn a_dollar_only_ever_lands_inside_a_string() {
        // A `$` outside a string would close the math span early and turn the
        // rest of the document into math. Inside one it is just a character —
        // Typst lexes the literal before it looks for a delimiter — which is why
        // it needs no escape and must not get one.
        let translated = math("\\text{a$b}");
        assert_eq!(translated, "upright(\"a$b\")");
        // Every `$` sits between the quotes of the string literal and nowhere
        // else: a delimiter outside one would close the math span early.
        let mut inside = false;
        for ch in translated.chars() {
            match ch {
                '"' => inside = !inside,
                '$' => assert!(inside, "a bare $ would close the math span: {translated}"),
                _ => {}
            }
        }
    }
}
