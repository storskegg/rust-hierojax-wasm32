//! The little JSON that batch mode needs: an object of strings, numbers, booleans and nulls in,
//! strings out. Nested values are an error.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

impl Value {
    pub fn write(&self, out: &mut String) {
        match self {
            Value::Str(s) => write_string(s, out),
            Value::Num(n) => out.push_str(&n.to_string()),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Null => out.push_str("null"),
        }
    }
}

/// A JSON string, quoted. Non-ASCII stays as UTF-8, which JSON allows.
pub fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
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
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl P<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, c: u8) -> Result<(), String> {
        self.ws();
        if self.s.get(self.i) == Some(&c) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("invalid JSON: expected '{}' at byte {}", c as char, self.i))
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let h = self.s.get(self.i..self.i + 4).ok_or("invalid JSON: short \\u escape")?;
        let h = std::str::from_utf8(h).map_err(|_| "invalid JSON: bad \\u escape".to_string())?;
        self.i += 4;
        u32::from_str_radix(h, 16).map_err(|_| "invalid JSON: bad \\u escape".to_string())
    }

    fn string(&mut self) -> Result<String, String> {
        self.eat(b'"')?;
        let mut out = Vec::new();
        loop {
            let b = *self.s.get(self.i).ok_or("invalid JSON: unterminated string")?;
            self.i += 1;
            match b {
                b'"' => break,
                b'\\' => {
                    let e = *self.s.get(self.i).ok_or("invalid JSON: unterminated string")?;
                    self.i += 1;
                    let c = match e {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let mut u = self.hex4()?;
                            if (0xD800..0xDC00).contains(&u) {
                                if self.s.get(self.i..self.i + 2) != Some(b"\\u") {
                                    return Err("invalid JSON: lone surrogate".to_string());
                                }
                                self.i += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err("invalid JSON: lone surrogate".to_string());
                                }
                                u = 0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00);
                            }
                            char::from_u32(u).ok_or("invalid JSON: lone surrogate")?
                        }
                        _ => return Err("invalid JSON: bad escape".to_string()),
                    };
                    let mut b4 = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut b4).as_bytes());
                }
                0..=0x1f => return Err("invalid JSON: control character in a string".to_string()),
                b => out.push(b),
            }
        }
        String::from_utf8(out).map_err(|_| "invalid UTF-8".to_string())
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.s.get(self.i) {
            Some(b'"') => Ok(Value::Str(self.string()?)),
            Some(b't') if self.s[self.i..].starts_with(b"true") => {
                self.i += 4;
                Ok(Value::Bool(true))
            }
            Some(b'f') if self.s[self.i..].starts_with(b"false") => {
                self.i += 5;
                Ok(Value::Bool(false))
            }
            Some(b'n') if self.s[self.i..].starts_with(b"null") => {
                self.i += 4;
                Ok(Value::Null)
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.i;
                while matches!(self.s.get(self.i), Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')) {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[start..self.i])
                    .ok()
                    .and_then(|t| t.parse::<f64>().ok())
                    .map(Value::Num)
                    .ok_or_else(|| "invalid JSON: bad number".to_string())
            }
            Some(b'{' | b'[') => Err("nested values are not accepted".to_string()),
            _ => Err(format!("invalid JSON: unexpected input at byte {}", self.i)),
        }
    }
}

/// One object, and nothing after it.
pub fn parse_object(line: &str) -> Result<Vec<(String, Value)>, String> {
    let mut p = P { s: line.as_bytes(), i: 0 };
    p.eat(b'{')?;
    let mut out = Vec::new();
    p.ws();
    if p.s.get(p.i) == Some(&b'}') {
        p.i += 1;
    } else {
        loop {
            let k = p.string()?;
            p.eat(b':')?;
            let v = p.value()?;
            out.push((k, v));
            p.ws();
            match p.s.get(p.i) {
                Some(b',') => p.i += 1,
                Some(b'}') => {
                    p.i += 1;
                    break;
                }
                _ => return Err(format!("invalid JSON: expected ',' or '}}' at byte {}", p.i)),
            }
        }
    }
    p.ws();
    if p.i != p.s.len() {
        return Err("invalid JSON: text after the object".to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_object() {
        let o = parse_object(r#" {"text":"aA𓀀\n","n":1.5e1,"b":true,"z":null} "#).expect("parse");
        assert_eq!(o[0], ("text".to_string(), Value::Str("aA\u{13000}\n".to_string())));
        assert_eq!(o[1].1, Value::Num(15.0));
        assert_eq!(o[2].1, Value::Bool(true));
        assert_eq!(o[3].1, Value::Null);
    }

    #[test]
    fn refuses_what_is_not_an_object_of_scalars() {
        for bad in ["", "[]", "{", r#"{"a":}"#, r#"{"a":[1]}"#, r#"{"a":1} x"#, r#"{"a":"\ud800"}"#, "{\"a\":\"\n\"}"] {
            assert!(parse_object(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn writes_strings() {
        let mut s = String::new();
        write_string("a\"b\\c\n\u{1}\u{13000}", &mut s);
        assert_eq!(s, "\"a\\\"b\\\\c\\n\\u0001\u{13000}\"");
    }
}
