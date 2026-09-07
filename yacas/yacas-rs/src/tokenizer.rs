//! Tokenizer. See upstream `cyacas/libyacas/src/tokenizer.cpp`
//! (`LispTokenizer`) and `xmltokenizer.cpp`.
//!
//! Token classes, in priority order:
//! comments (`/* */` and `//`), brackets `() {} []`, `%`, `,` `;`,
//! `.`/`..`, strings (quotes kept, escapes decoded), atoms
//! (alphabetic + apostrophe), symbolic operators, `_` subscripts, numbers
//! (`e`/`E` exponents).
//!
//! Notes:
//! - Tokens are returned verbatim; interning is the parser's job.
//! - Alphabetic test: C++ enumerates Lu+Ll code points and Java uses
//!   `Character.isAlphabetic`; Rust uses `char::is_alphabetic()` (a superset,
//!   also covering Lt/Lm/Lo). Covers ASCII and common Greek letters.
//! - Input is held as `Vec<char>` and `pos` is a char index, matching the
//!   upstream character-offset semantics (backtracking stays exact).

/// Lexical errors.
#[derive(Debug)]
pub enum TokenError {
    /// `/*` not closed before EOF (upstream `KLispErrCommentToEndOfFile`).
    CommentToEndOfFile,
    /// Unterminated string / invalid escape (upstream `KLispErrParsingInput`).
    ParsingInput,
    /// Token fits no class (upstream `InvalidToken`).
    InvalidToken,
}

pub struct Tokenizer {
    chars: Vec<char>,
    pos: usize,
    /// XML tokenizer mode (see upstream `xmltokenizer.cpp`), toggled by the
    /// `XmlTokenizer()` command and restored by `DefaultTokenizer()`.
    pub xml: bool,
}

impl Tokenizer {
    pub fn new(src: &str) -> Self {
        Tokenizer {
            chars: src.chars().collect(),
            pos: 0,
            xml: false,
        }
    }

    pub fn end_of_stream(&self) -> bool {
        self.pos >= self.chars.len()
    }

    /// Next character without advancing; `'\0'` at EOF (inputs contain no NUL).
    pub fn peek(&self) -> char {
        if self.end_of_stream() {
            '\0'
        } else {
            self.chars[self.pos]
        }
    }

    /// Consume and return the next character.
    // (Name mirrors the upstream API; the `Iterator::next` similarity is
    // intentional but this is not an iterator.)
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> char {
        let c = if self.end_of_stream() {
            '\0'
        } else {
            self.chars[self.pos]
        };
        if !self.end_of_stream() {
            self.pos += 1;
        }
        c
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    /// Backtrack (used by operator splitting); see upstream
    /// `SetPosition`.
    pub fn set_position(&mut self, p: usize) {
        self.pos = p;
    }

    /// Current input line number: consumed-newline count + 1 (read by
    /// `CurrentLine`).
    pub fn line(&self) -> u32 {
        1 + self.chars[..self.pos]
            .iter()
            .filter(|&&c| c == '\n')
            .count() as u32
    }

    /// Next token; an empty string means EOF (upstream convention).
    pub fn next_token(&mut self) -> Result<String, TokenError> {
        if self.xml {
            return self.next_token_xml();
        }
        // Skip whitespace and comments; `c` holds the first significant char.
        let c: char;
        loop {
            if self.end_of_stream() {
                return Ok(String::new());
            }
            let cc = self.next();
            if cc.is_whitespace() {
                continue;
            }
            if cc == '/' && self.peek() == '*' {
                self.next();
                loop {
                    while self.next() != '*' && !self.end_of_stream() {}
                    if self.end_of_stream() {
                        return Err(TokenError::CommentToEndOfFile);
                    }
                    if self.peek() == '/' {
                        self.next();
                        break;
                    }
                }
                continue;
            }
            if cc == '/' && self.peek() == '/' {
                self.next();
                while self.next() != '\n' && !self.end_of_stream() {}
                continue;
            }
            c = cc;
            break;
        }

        if matches!(c, '(' | ')' | '{' | '}' | '[' | ']') {
            return Ok(c.to_string());
        }
        if c == '%' {
            return Ok("%".into());
        }
        if matches!(c, ',' | ';') {
            return Ok(c.to_string());
        }
        // `.` or `..` (when not starting a number)
        if c == '.' && !self.peek().is_ascii_digit() {
            let mut t = String::new();
            t.push(c);
            while self.peek() == '.' {
                t.push(self.next());
            }
            return Ok(t);
        }
        // String: returned with its quotes; `\" \\ \t \n` decoded inside.
        if c == '"' {
            let mut s = String::new();
            s.push(c);
            while self.peek() != '"' {
                if self.peek() == '\\' {
                    self.next();
                    if self.end_of_stream() {
                        return Err(TokenError::ParsingInput);
                    }
                    match self.next() {
                        '"' => s.push('"'),
                        '\\' => s.push('\\'),
                        't' => s.push('\t'),
                        'n' => s.push('\n'),
                        _ => return Err(TokenError::ParsingInput),
                    }
                } else {
                    s.push(self.next());
                }
                if self.end_of_stream() {
                    return Err(TokenError::ParsingInput);
                }
            }
            s.push(self.next());
            return Ok(s);
        }
        // Atom: starts alphabetic, continues alphanumeric + apostrophe.
        if is_alpha(c) {
            let mut a = String::new();
            a.push(c);
            while is_alpha(self.peek()) || self.peek().is_ascii_digit() {
                a.push(self.next());
            }
            return Ok(a);
        }
        // Symbolic operator (greedy).
        if is_symbolic(c) {
            let mut op = String::new();
            op.push(c);
            while is_symbolic(self.peek()) {
                op.push(self.next());
            }
            return Ok(op);
        }
        // Subscript underscore (greedy `__` sequence).
        if c == '_' {
            let mut t = String::new();
            t.push(c);
            while self.peek() == '_' {
                t.push(self.next());
            }
            return Ok(t);
        }
        // Number: digits [. digits] [e|E [+|-] digits]
        if c.is_ascii_digit() || c == '.' {
            let mut n = String::new();
            n.push(c);
            while self.peek().is_ascii_digit() {
                n.push(self.next());
            }
            if self.peek() == '.' {
                n.push(self.next());
                while self.peek().is_ascii_digit() {
                    n.push(self.next());
                }
            }
            if self.peek() == 'e' || self.peek() == 'E' {
                n.push(self.next());
                if self.peek() == '-' || self.peek() == '+' {
                    n.push(self.next());
                }
                if !self.peek().is_ascii_digit() {
                    return Err(TokenError::ParsingInput);
                }
                while self.peek().is_ascii_digit() {
                    n.push(self.next());
                }
            }
            return Ok(n);
        }
        Err(TokenError::InvalidToken)
    }

    /// XML mode: collect leading whitespace; `<` consumes through `>`
    /// (unterminated → `CommentToEndOfFile`); otherwise consume up to the
    /// next `<` and prepend the leading whitespace.
    fn next_token_xml(&mut self) -> Result<String, TokenError> {
        if self.end_of_stream() {
            return Ok(String::new());
        }
        let mut leading = String::new();
        while self.peek().is_ascii_whitespace() {
            leading.push(self.next());
        }
        if self.end_of_stream() {
            return Ok(String::new());
        }
        let mut s = String::new();
        let mut c = self.next();
        s.push(c);
        if c == '<' {
            while c != '>' {
                if self.end_of_stream() {
                    return Err(TokenError::CommentToEndOfFile);
                }
                c = self.next();
                s.push(c);
            }
        } else {
            while self.peek() != '<' && !self.end_of_stream() {
                s.push(self.next());
            }
            s = leading + &s;
        }
        Ok(s)
    }
}

/// Alphabetic (Unicode) or apostrophe.
pub fn is_alpha(c: char) -> bool {
    c == '\'' || c.is_alphabetic()
}

/// Symbolic-operator alphabet (same set as upstream; `%` and `_` excluded).
const SYMBOLICS: &str = "~`!@#$^&*-=+:<>?/\\|";

pub fn is_symbolic(c: char) -> bool {
    SYMBOLICS.contains(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(s: &str) -> Vec<String> {
        let mut t = Tokenizer::new(s);
        let mut v = Vec::new();
        loop {
            let tok = t.next_token().expect("tokenize");
            if tok.is_empty() {
                break;
            }
            v.push(tok);
        }
        v
    }

    #[test]
    fn arithmetic_operators_split() {
        assert_eq!(tokens("1+2*3"), vec!["1", "+", "2", "*", "3"]);
        assert_eq!(tokens("aa<=bb"), vec!["aa", "<=", "bb"]);
        assert_eq!(tokens("aa<-bb"), vec!["aa", "<-", "bb"]);
    }

    #[test]
    fn greedy_symbolic_and_split() {
        // Without whitespace `*-` lexes as one token; the parser splits it.
        assert_eq!(tokens("aa*-bb"), vec!["aa", "*-", "bb"]);
        assert_eq!(tokens("aa!+bb"), vec!["aa", "!+", "bb"]);
    }

    #[test]
    fn comments_skipped() {
        // A comment directly after a symbolic operator is absorbed by the
        // greedy scan (`+/*` lexes as one token), so upstream syntax demands
        // whitespace before a comment.
        assert_eq!(tokens("aa + /* c */ bb"), vec!["aa", "+", "bb"]);
        assert_eq!(tokens("aa// c\n+bb"), vec!["aa", "+", "bb"]);
    }

    #[test]
    fn string_keeps_quotes_and_escapes() {
        assert_eq!(tokens("\"hello\""), vec!["\"hello\""]);
        // `\"` decodes to `"` (printed verbatim by FullForm output).
        assert_eq!(tokens("\"a\\\"b\""), vec!["\"a\"b\""]);
        assert_eq!(tokens("\"a\\\\b\""), vec!["\"a\\b\""]);
        assert_eq!(tokens("\"a\\tb\""), vec!["\"a\tb\""]);
    }

    #[test]
    fn underscore_and_dots() {
        assert_eq!(tokens("aa_1"), vec!["aa", "_", "1"]);
        assert_eq!(tokens("aa__"), vec!["aa", "__"]);
        assert_eq!(tokens("aa..bb"), vec!["aa", "..", "bb"]);
    }

    #[test]
    fn numbers_with_exponent() {
        assert_eq!(tokens("1e10"), vec!["1e10"]);
        assert_eq!(tokens("2.5e-3"), vec!["2.5e-3"]);
        assert_eq!(tokens(".5"), vec![".5"]);
        assert_eq!(tokens("12345.678"), vec!["12345.678"]);
        // `+-` lexes as a single token (upstream syntax).
        assert_eq!(tokens("aa+-bb"), vec!["aa", "+-", "bb"]);
    }

    #[test]
    fn exponent_requires_digits() {
        for source in ["1e", "1E", "1e+", "1e-", "1.e+"] {
            let mut tokenizer = Tokenizer::new(source);
            assert!(matches!(
                tokenizer.next_token(),
                Err(TokenError::ParsingInput)
            ));
        }
    }

    #[test]
    fn unterminated_constructions() {
        let mut t = Tokenizer::new("aa /* x");
        assert_eq!(t.next_token().unwrap(), "aa");
        assert!(matches!(
            t.next_token(),
            Err(TokenError::CommentToEndOfFile)
        ));
        let mut t2 = Tokenizer::new("\"abc");
        assert!(matches!(t2.next_token(), Err(TokenError::ParsingInput)));
    }
}
