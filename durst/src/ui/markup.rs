//! The body markup of the notification spec: `<b> <i> <u> <a href> <img alt>`
//! plus `<br>` and XML entities. Unknown tags are dropped; anything that
//! doesn't look like a tag is kept as text, so plain text survives too.

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub link: Option<String>,
}

impl Run {
    fn same_format(&self, other: &Run) -> bool {
        (self.bold, self.italic, self.underline, &self.link)
            == (other.bold, other.italic, other.underline, &other.link)
    }
}

#[derive(Default)]
struct Format {
    bold: u32,
    italic: u32,
    underline: u32,
    links: Vec<String>,
}

pub fn parse(input: &str) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    let mut format = Format::default();
    let mut push = |text: &str, f: &Format| {
        if text.is_empty() {
            return;
        }
        let run = Run {
            text: text.to_owned(),
            bold: f.bold > 0,
            italic: f.italic > 0,
            underline: f.underline > 0,
            link: f.links.last().cloned(),
        };
        match runs.last_mut() {
            Some(last) if last.same_format(&run) => last.text.push_str(text),
            _ => runs.push(run),
        }
    };

    let mut rest = input;
    while let Some(i) = rest.find(['<', '&']) {
        push(&rest[..i], &format);
        rest = &rest[i..];
        if rest.starts_with('&') {
            match entity(rest) {
                Some((c, len)) => {
                    push(c.encode_utf8(&mut [0; 4]), &format);
                    rest = &rest[len..];
                }
                None => {
                    push("&", &format);
                    rest = &rest[1..];
                }
            }
            continue;
        }
        let Some(tag) = tag(rest) else {
            push("<", &format);
            rest = &rest[1..];
            continue;
        };
        rest = &rest[tag.len..];
        let counter = match tag.name.as_str() {
            "b" => Some(&mut format.bold),
            "i" => Some(&mut format.italic),
            "u" => Some(&mut format.underline),
            _ => None,
        };
        match (tag.name.as_str(), tag.closing) {
            (_, false) if tag.self_closing && counter.is_some() => {}
            (_, false) if counter.is_some() => *counter.unwrap() += 1,
            (_, true) if counter.is_some() => {
                let c = counter.unwrap();
                *c = c.saturating_sub(1);
            }
            ("a", false) => format
                .links
                .push(attribute(&tag.attrs, "href").unwrap_or_default()),
            ("a", true) => {
                format.links.pop();
            }
            ("img", false) => push(&attribute(&tag.attrs, "alt").unwrap_or_default(), &format),
            ("br", _) => push("\n", &format),
            _ => {}
        }
    }
    push(rest, &format);
    runs
}

/// The first link of the body, for the `open_url` mouse action.
pub fn first_link(runs: &[Run]) -> Option<&str> {
    runs.iter()
        .find_map(|r| r.link.as_deref().filter(|l| !l.is_empty()))
}

struct Tag {
    name: String,
    attrs: String,
    closing: bool,
    self_closing: bool,
    /// bytes including `<` and `>`
    len: usize,
}

/// Parses `<name attrs>`, `</name>` or `<name/>` at the start of `s`.
fn tag(s: &str) -> Option<Tag> {
    let end = s.find('>')?;
    let inner = &s[1..end];
    if inner.contains('<') {
        return None;
    }
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(inner) => (true, inner),
        None => (false, inner),
    };
    let (self_closing, inner) = match inner.strip_suffix('/') {
        Some(inner) => (true, inner),
        None => (false, inner),
    };
    let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
    let name = &inner[..name_end];
    let valid = name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    if !valid || (closing && name_end != inner.len()) {
        return None;
    }
    Some(Tag {
        name: name.to_ascii_lowercase(),
        attrs: inner[name_end..].to_owned(),
        closing,
        self_closing,
        len: end + 1,
    })
}

/// The value of `name="..."` or `name='...'`, with entities decoded.
fn attribute(attrs: &str, name: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(i) = rest.find(name) {
        let before_ok = rest[..i].ends_with(char::is_whitespace);
        let after = rest[i + name.len()..].trim_start();
        if let (true, Some(after)) = (before_ok, after.strip_prefix('=')) {
            let after = after.trim_start();
            let quote = after.chars().next().filter(|q| *q == '"' || *q == '\'')?;
            let value = &after[1..];
            let value = &value[..value.find(quote)?];
            return Some(parse(value).into_iter().map(|r| r.text).collect());
        }
        rest = &rest[i + name.len()..];
    }
    None
}

/// Decodes `&name;`, `&#NN;` or `&#xHH;` at the start of `s`.
fn entity(s: &str) -> Option<(char, usize)> {
    let end = s[..s.len().min(12)].find(';')?;
    let name = &s[1..end];
    let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
            let num = name.strip_prefix('#')?;
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => num.parse().ok()?,
            };
            char::from_u32(code)?
        }
    };
    Some((c, end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str) -> Run {
        Run {
            text: text.into(),
            ..Run::default()
        }
    }

    #[test]
    fn plain_text_is_one_run() {
        assert_eq!(parse("hello world"), vec![run("hello world")]);
        assert_eq!(parse(""), vec![]);
    }

    #[test]
    fn formatting_nests() {
        let runs = parse("a <b>b <i>bi</i></b> <u>u</u>");
        let f = |r: &Run| (r.text.clone(), r.bold, r.italic, r.underline);
        assert_eq!(
            runs.iter().map(f).collect::<Vec<_>>(),
            [
                ("a ", false, false, false),
                ("b ", true, false, false),
                ("bi", true, true, false),
                (" ", false, false, false),
                ("u", false, false, true),
            ]
            .map(|(t, b, i, u)| (t.to_owned(), b, i, u))
        );
    }

    #[test]
    fn links_and_images() {
        let runs = parse(
            r#"see <a href="https://a.org/?x=1&amp;y=2">here</a> <img src="x.png" alt="[pic]"/>"#,
        );
        assert_eq!(runs[1].text, "here");
        assert_eq!(runs[1].link.as_deref(), Some("https://a.org/?x=1&y=2"));
        assert_eq!(runs[2].text, " [pic]");
        assert_eq!(first_link(&runs), Some("https://a.org/?x=1&y=2"));
        assert_eq!(first_link(&parse("none")), None);
    }

    #[test]
    fn entities() {
        assert_eq!(
            parse("a &lt;b&gt; &amp; &#65;&#x42; &bogus; &"),
            vec![run("a <b> & AB &bogus; &")]
        );
    }

    #[test]
    fn non_tags_are_text() {
        assert_eq!(parse("1 < 2 and 3 > 2"), vec![run("1 < 2 and 3 > 2")]);
        assert_eq!(parse("a<b"), vec![run("a<b")]);
        assert_eq!(parse("x <-> y"), vec![run("x <-> y")]);
    }

    #[test]
    fn unknown_and_unbalanced_tags_are_dropped() {
        assert_eq!(
            parse("<span foo='x'>a</span></b>b<br>c"),
            vec![run("ab\nc")]
        );
        assert!(parse("<b>bold to the end").iter().all(|r| r.bold));
    }
}
