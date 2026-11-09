//! A JSON value and its one serialization: keys in the order given, no
//! whitespace, numbers in Rust's shortest round-trip form (the same on
//! every host).

/// A JSON value. Objects keep their keys in the order given.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Int(u64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(&'static str, Json)>),
}

impl Json {
    /// Writes the value. Numbers must be finite (the callers check); `-0`
    /// is written `0`.
    pub(crate) fn write(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Num(v) => {
                let v = if *v == 0.0 { 0.0 } else { *v };
                debug_assert!(v.is_finite());
                out.push_str(&format!("{v}"));
            }
            Json::Int(v) => out.push_str(&v.to_string()),
            Json::Str(s) => string(s, out),
            Json::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Obj(fields) => {
                out.push('{');
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    string(k, out);
                    out.push(':');
                    v.write(out);
                }
                out.push('}');
            }
        }
    }
}

/// A number from an f32 value: written so that it reads back as that f32.
pub(crate) fn f32n(v: f32) -> Json {
    Json::Num(f64::from(v))
}

/// An array of numbers.
pub(crate) fn nums(v: &[f64]) -> Json {
    Json::Arr(v.iter().map(|&x| Json::Num(x)).collect())
}

fn string(s: &str, out: &mut String) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_compact_ordered_json() {
        let v = Json::Obj(vec![
            ("b", Json::Num(-0.0)),
            ("a", Json::Arr(vec![Json::Num(0.1), Json::Int(3)])),
            ("s", Json::Str("q\"\\\u{1}".into())),
        ]);
        let mut s = String::new();
        v.write(&mut s);
        assert_eq!(s, r#"{"b":0,"a":[0.1,3],"s":"q\"\\\u0001"}"#);
    }
}
