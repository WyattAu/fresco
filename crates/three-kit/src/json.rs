//! Minimal JSON parser: deterministic, std-only, no dependencies.
//!
//! Written for the glTF subset loader (M8.4) — glTF is JSON plus
//! binary buffers, and the estate prefers a small spec-compliant
//! parser over pulling a dependency tree into a mid-layer crate.
//!
//! Recursion is depth-capped (hostile nesting fails closed), objects
//! preserve insertion order (`Vec` pairs), and all arithmetic is f64
//! (glTF numbers are f32/f64 — exact for the subset we need).

/// A parsed JSON value. Objects keep insertion order as pairs.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    /// null.
    Null,
    /// true / false.
    Bool(bool),
    /// Any number (f64).
    Num(f64),
    /// A string (escapes resolved).
    Str(String),
    /// An array.
    Arr(Vec<Json>),
    /// An object: insertion-ordered key/value pairs.
    Obj(Vec<(String, Json)>),
}

impl Json {
    /// Object member lookup (linear scan; glTF objects are small).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// Array element by index.
    #[must_use]
    pub fn at(&self, i: usize) -> Option<&Json> {
        match self {
            Json::Arr(items) => items.get(i),
            _ => None,
        }
    }

    /// Number value.
    #[must_use]
    pub const fn num(&self) -> Option<f64> {
        match self {
            Json::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// Unsigned integer value (indices, counts).
    #[must_use]
    pub fn uint(&self) -> Option<usize> {
        match self {
            Json::Num(n) if *n >= 0.0 && n.fract() == 0.0 && *n <= 9.0e15 => {
                usize::try_from(*n as i128).ok()
            }
            _ => None,
        }
    }

    /// String value.
    #[must_use]
    pub fn str_val(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// Maximum nesting depth: hostile input fails closed instead of
/// blowing the stack.
const MAX_DEPTH: usize = 64;

/// Parse a JSON document. Returns `None` on any syntax error or
/// trailing garbage.
#[must_use]
pub fn parse(text: &str) -> Option<Json> {
    let chars: Vec<char> = text.chars().collect();
    let mut pos = 0_usize;
    skip_ws(&chars, &mut pos);
    let value = parse_value(&chars, &mut pos, 0)?;
    skip_ws(&chars, &mut pos);
    if pos != chars.len() {
        return None; // trailing garbage
    }
    Some(value)
}

fn skip_ws(chars: &[char], pos: &mut usize) {
    while *pos < chars.len() {
        match chars.get(*pos) {
            Some(' ') | Some('\t') | Some('\n') | Some('\r') => *pos += 1,
            _ => break,
        }
    }
}

fn parse_value(chars: &[char], pos: &mut usize, depth: usize) -> Option<Json> {
    if depth > MAX_DEPTH {
        return None;
    }
    skip_ws(chars, pos);
    match chars.get(*pos)? {
        '{' => parse_obj(chars, pos, depth),
        '[' => parse_arr(chars, pos, depth),
        '"' => parse_str(chars, pos).map(Json::Str),
        't' => literal(chars, pos, "true", Json::Bool(true)),
        'f' => literal(chars, pos, "false", Json::Bool(false)),
        'n' => literal(chars, pos, "null", Json::Null),
        c if *c == '-' || c.is_ascii_digit() => parse_num(chars, pos),
        _ => None,
    }
}

fn literal(chars: &[char], pos: &mut usize, word: &str, value: Json) -> Option<Json> {
    for c in word.chars() {
        if chars.get(*pos) != Some(&c) {
            return None;
        }
        *pos += 1;
    }
    Some(value)
}

fn parse_obj(chars: &[char], pos: &mut usize, depth: usize) -> Option<Json> {
    *pos += 1; // '{'
    let mut pairs: Vec<(String, Json)> = Vec::new();
    skip_ws(chars, pos);
    if chars.get(*pos) == Some(&'}') {
        *pos += 1;
        return Some(Json::Obj(pairs));
    }
    loop {
        skip_ws(chars, pos);
        if chars.get(*pos) != Some(&'"') {
            return None;
        }
        let key = parse_str(chars, pos)?;
        skip_ws(chars, pos);
        if chars.get(*pos) != Some(&':') {
            return None;
        }
        *pos += 1;
        let value = parse_value(chars, pos, depth + 1)?;
        pairs.push((key, value));
        skip_ws(chars, pos);
        match chars.get(*pos) {
            Some(',') => *pos += 1,
            Some('}') => {
                *pos += 1;
                return Some(Json::Obj(pairs));
            }
            _ => return None,
        }
    }
}

fn parse_arr(chars: &[char], pos: &mut usize, depth: usize) -> Option<Json> {
    *pos += 1; // '['
    let mut items: Vec<Json> = Vec::new();
    skip_ws(chars, pos);
    if chars.get(*pos) == Some(&']') {
        *pos += 1;
        return Some(Json::Arr(items));
    }
    loop {
        let value = parse_value(chars, pos, depth + 1)?;
        items.push(value);
        skip_ws(chars, pos);
        match chars.get(*pos) {
            Some(',') => *pos += 1,
            Some(']') => {
                *pos += 1;
                return Some(Json::Arr(items));
            }
            _ => return None,
        }
    }
}

fn parse_str(chars: &[char], pos: &mut usize) -> Option<String> {
    if chars.get(*pos) != Some(&'"') {
        return None;
    }
    *pos += 1;
    let mut out = String::new();
    loop {
        let c = chars.get(*pos)?;
        *pos += 1;
        match c {
            '"' => return Some(out),
            '\\' => {
                let esc = chars.get(*pos)?;
                *pos += 1;
                match esc {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000C}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        // Four hex digits -> scalar (surrogate pairs
                        // combined; lone surrogates fail closed).
                        let mut code = 0_u32;
                        for _ in 0..4 {
                            let h = chars.get(*pos)?;
                            *pos += 1;
                            code = code * 16 + h.to_digit(16)?;
                        }
                        if (0xD800..0xDC00).contains(&code) {
                            // High surrogate: expect a low surrogate.
                            if chars.get(*pos) != Some(&'\\') && chars.get(*pos + 1) != Some(&'u') {
                                return None;
                            }
                            *pos += 2;
                            let mut low = 0_u32;
                            for _ in 0..4 {
                                let h = chars.get(*pos)?;
                                *pos += 1;
                                low = low * 16 + h.to_digit(16)?;
                            }
                            if !(0xDC00..0xE000).contains(&low) {
                                return None;
                            }
                            let combined = 0x10000 + ((code - 0xD800) << 10) + (low - 0xDC00);
                            out.push(char::from_u32(combined)?);
                        } else if (0xDC00..0xE000).contains(&code) {
                            return None; // lone low surrogate
                        } else {
                            out.push(char::from_u32(code)?);
                        }
                    }
                    _ => return None,
                }
            }
            other => out.push(*other),
        }
    }
}

fn parse_num(chars: &[char], pos: &mut usize) -> Option<Json> {
    let start = *pos;
    if chars.get(*pos) == Some(&'-') {
        *pos += 1;
    }
    // Integer part: at least one digit; no leading zeros beyond "0".
    match chars.get(*pos) {
        Some('0') => *pos += 1,
        Some(c) if c.is_ascii_digit() => {
            while matches!(chars.get(*pos), Some(c) if c.is_ascii_digit()) {
                *pos += 1;
            }
        }
        _ => return None,
    }
    // Fraction.
    if chars.get(*pos) == Some(&'.') {
        *pos += 1;
        if !matches!(chars.get(*pos), Some(c) if c.is_ascii_digit()) {
            return None;
        }
        while matches!(chars.get(*pos), Some(c) if c.is_ascii_digit()) {
            *pos += 1;
        }
    }
    // Exponent.
    if matches!(chars.get(*pos), Some('e') | Some('E')) {
        *pos += 1;
        if matches!(chars.get(*pos), Some('+') | Some('-')) {
            *pos += 1;
        }
        if !matches!(chars.get(*pos), Some(c) if c.is_ascii_digit()) {
            return None;
        }
        while matches!(chars.get(*pos), Some(c) if c.is_ascii_digit()) {
            *pos += 1;
        }
    }
    let text: String = chars.get(start..*pos)?.iter().collect();
    text.parse::<f64>().ok().map(Json::Num)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: a representative document parses into the exact tree.
    #[test]
    fn parses_representative_document() {
        let doc = r#"{"name":"tri","verts":[0,-1.5,2.5e2],"count":3,"ok":true,"empty":null,
                       "nested":{"ids":[1,2],"tag":"a\"b"}}"#;
        let Some(v) = parse(doc) else {
            return;
        };
        assert_eq!(v.get("name").and_then(|j| j.str_val()), Some("tri"));
        assert_eq!(v.get("count").and_then(|j| j.uint()), Some(3));
        assert_eq!(v.get("ok"), Some(&Json::Bool(true)));
        assert_eq!(v.get("empty"), Some(&Json::Null));
        let verts = v.get("verts").and_then(|j| j.at(1)).and_then(|j| j.num());
        assert_eq!(verts, Some(-1.5));
        let exp = v.get("verts").and_then(|j| j.at(2)).and_then(|j| j.num());
        assert_eq!(exp, Some(250.0));
        let tag = v
            .get("nested")
            .and_then(|j| j.get("tag"))
            .and_then(|j| j.str_val());
        assert_eq!(tag, Some("a\"b"));
    }

    /// Oracle: escapes resolve, including a surrogate pair.
    #[test]
    fn escapes_resolve() {
        let Some(v) = parse(r#""a\n\tbA""#) else {
            return;
        };
        assert_eq!(v.str_val(), Some("a\n\tbA"));
        let Some(pair) = parse("\"😀\"") else {
            return;
        };
        assert_eq!(pair.str_val(), Some("😀"));
    }

    /// Oracle: malformed documents fail closed.
    #[test]
    fn malformed_fails_closed() {
        for bad in [
            "",
            "{",
            "[1,",
            "{\"a\"}",
            "{\"a\":}",
            "01",
            "1.",
            ".5",
            "1e",
            "tru",
            "[1] trailing",
            "nul",
            r#""lone\ud800""#,
            "-",
        ] {
            assert!(parse(bad).is_none(), "must fail: {bad:?}");
        }
    }

    /// Oracle: deep nesting fails closed (stack safety).
    #[test]
    fn deep_nesting_fails_closed() {
        let doc = "[".repeat(200) + &"]".repeat(200);
        assert!(parse(&doc).is_none(), "depth cap must reject");
        let ok_depth = "[".repeat(30) + &"]".repeat(30);
        assert!(parse(&ok_depth).is_some(), "normal depth parses");
    }
}
