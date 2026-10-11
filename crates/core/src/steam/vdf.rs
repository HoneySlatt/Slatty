//! Steam's text KeyValues files (`.vdf`, `.acf`): quoted keys and values, sections in braces.
//! Written by the Steam client, read here only; a file that does not parse is left out.

use crate::error::{Error, Result};

/// A value: text, or a section of keys in the file's order.
#[derive(Debug, Clone, PartialEq)]
pub enum Vdf {
    Text(String),
    Section(Vec<(String, Vdf)>),
}

/// Sections deeper than this are refused: Steam's files nest a few levels.
const MAX_DEPTH: usize = 32;

impl Vdf {
    /// The value of `key` in this section, compared without case as Steam does.
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        self.entries()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    /// The text of `key` in this section.
    pub fn text(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Vdf::Text(t) => Some(t),
            Vdf::Section(_) => None,
        }
    }

    /// The keys of a section, none for text.
    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Section(entries) => entries,
            Vdf::Text(_) => &[],
        }
    }
}

/// The keys of a file, as a section.
pub fn parse(text: &str) -> Result<Vdf> {
    let mut tokens = Tokens {
        chars: text.chars().peekable(),
    };
    let entries = section(&mut tokens, 0, false)?;
    Ok(Vdf::Section(entries))
}

fn section(tokens: &mut Tokens<'_>, depth: usize, braced: bool) -> Result<Vec<(String, Vdf)>> {
    let bad = |what: &str| Error::parse("a Steam file", what.to_string());
    if depth > MAX_DEPTH {
        return Err(bad("nested too deep"));
    }
    let mut entries = Vec::new();
    loop {
        let key = match tokens.next()? {
            None if !braced => return Ok(entries),
            None => return Err(bad("a section is not closed")),
            Some(Token::Close) if braced => return Ok(entries),
            Some(Token::Text(key)) => key,
            Some(_) => return Err(bad("a key was expected")),
        };
        let value = match tokens.next()? {
            Some(Token::Open) => Vdf::Section(section(tokens, depth + 1, true)?),
            Some(Token::Text(value)) => Vdf::Text(value),
            _ => return Err(bad("a value was expected")),
        };
        entries.push((key, value));
    }
}

enum Token {
    Text(String),
    Open,
    Close,
}

struct Tokens<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
}

impl Tokens<'_> {
    /// The next token, past spaces, `//` comments and the `[$WIN32]`-like conditions some files
    /// put after a value.
    fn next(&mut self) -> Result<Option<Token>> {
        loop {
            match self.chars.peek() {
                None => return Ok(None),
                Some(c) if c.is_whitespace() => {
                    self.chars.next();
                }
                Some('/') => {
                    for c in self.chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                }
                Some('[') => {
                    for c in self.chars.by_ref() {
                        if c == ']' {
                            break;
                        }
                    }
                }
                Some('{') => {
                    self.chars.next();
                    return Ok(Some(Token::Open));
                }
                Some('}') => {
                    self.chars.next();
                    return Ok(Some(Token::Close));
                }
                Some('"') => {
                    self.chars.next();
                    return self.quoted().map(|t| Some(Token::Text(t)));
                }
                Some(_) => {
                    let mut text = String::new();
                    while let Some(&c) = self.chars.peek() {
                        if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                            break;
                        }
                        text.push(c);
                        self.chars.next();
                    }
                    return Ok(Some(Token::Text(text)));
                }
            }
        }
    }

    fn quoted(&mut self) -> Result<String> {
        let mut text = String::new();
        while let Some(c) = self.chars.next() {
            match c {
                '"' => return Ok(text),
                '\\' => match self.chars.next() {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some(other) => text.push(other),
                    None => break,
                },
                c => text.push(c),
            }
        }
        Err(Error::parse("a Steam file", "a quote is not closed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_files_are_read_as_written() {
        let vdf = parse(
            r#"
"libraryfolders"
{
	// a comment
	"0"
	{
		"path"		"/home/someone/.local/share/Steam"
		"apps"
		{
			"440"		"23412"
		}
	}
	"1"	{ "path" "D:\\Games\\Steam" "label" "a \"quoted\" name" }
	"OSList" "windows" [$WIN32]
}
"#,
        )
        .unwrap();
        let folders = vdf.get("LibraryFolders").unwrap();
        let first = folders.get("0").unwrap();
        assert_eq!(first.text("path"), Some("/home/someone/.local/share/Steam"));
        assert_eq!(first.get("apps").unwrap().text("440"), Some("23412"));
        let second = folders.get("1").unwrap();
        assert_eq!(second.text("path"), Some("D:\\Games\\Steam"));
        assert_eq!(second.text("label"), Some("a \"quoted\" name"));
        assert_eq!(folders.text("oslist"), Some("windows"));
    }

    #[test]
    fn broken_or_hostile_files_are_refused() {
        for text in [
            "\"a\" {",
            "\"a\" \"unclosed",
            "{ }",
            "\"a\" }",
            &"\"a\" {".repeat(MAX_DEPTH + 2),
        ] {
            assert!(parse(text).is_err(), "{text:?}");
        }
    }
}
