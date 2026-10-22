//! TOON at the process boundary. The CLI emits objects, scalar arrays and uniform tables.
use serde_json::Value;

fn quoted(s: &str) -> String {
    use std::fmt::Write;
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0000}'..='\u{001f}' => write!(out, "\\u{:04x}", c as u32).unwrap(),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// TOON §7.2's numeric-like grammar is deliberately broader than JSON numbers:
// leading zeros and a leading plus still need quoting to preserve string type.
fn numeric_like(s: &str) -> bool {
    fn digits(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
    }
    fn unsigned(s: &str) -> &str {
        s.strip_prefix(['+', '-']).unwrap_or(s)
    }
    let mut exponent = unsigned(s).split(['e', 'E']);
    let mut mantissa = exponent.next().unwrap().split('.');
    if !digits(mantissa.next().unwrap()) {
        return false;
    }
    if let Some(fraction) = mantissa.next()
        && !digits(fraction)
    {
        return false;
    }
    if mantissa.next().is_some() {
        return false;
    }
    if let Some(power) = exponent.next()
        && !digits(unsigned(power))
    {
        return false;
    }
    exponent.next().is_none()
}
fn string(s: &str) -> String {
    let needs_quotes = s.is_empty()
        || s.starts_with([' ', '\t', '-', '#'])
        || s.ends_with([' ', '\t'])
        || matches!(s, "true" | "false" | "null")
        || numeric_like(s)
        || s.chars().any(|c| {
            c <= '\u{001f}' || matches!(c, ':' | '"' | '\\' | '[' | ']' | '{' | '}' | ',')
        });
    if needs_quotes { quoted(s) } else { s.into() }
}
fn encoded_key(s: &str) -> String {
    let mut chars = s.bytes();
    if chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
    {
        s.into()
    } else {
        quoted(s)
    }
}
fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => string(s),
        _ => v.to_string(),
    }
}
pub fn encode(value: &Value) -> String {
    let mut out = String::new();
    object(
        value.as_object().expect("output root must be an object"),
        0,
        &mut out,
    );
    out.pop(); // Encoder output has no trailing LF; print() adds the CLI line terminator.
    out
}
fn object(obj: &serde_json::Map<String, Value>, depth: usize, out: &mut String) {
    for (key, v) in obj {
        let indent = "  ".repeat(depth);
        let key = encoded_key(key);
        match v {
            Value::Object(m) => {
                out.push_str(&format!("{indent}{key}:\n"));
                object(m, depth + 1, out);
            }
            Value::Array(a) if a.is_empty() => out.push_str(&format!("{indent}{key}: []\n")),
            Value::Array(a) if a.iter().all(|x| !x.is_object() && !x.is_array()) => {
                out.push_str(&format!(
                    "{indent}{key}[{}]: {}\n",
                    a.len(),
                    a.iter().map(scalar).collect::<Vec<_>>().join(",")
                ));
            }
            Value::Array(a) => {
                let fields: Vec<_> = a[0].as_object().expect("table row").keys().collect();
                out.push_str(&format!(
                    "{indent}{key}[{}]{{{}}}:\n",
                    a.len(),
                    fields
                        .iter()
                        .map(|k| encoded_key(k))
                        .collect::<Vec<_>>()
                        .join(",")
                ));
                for row in a {
                    assert_eq!(
                        row.as_object()
                            .expect("table row")
                            .keys()
                            .collect::<Vec<_>>(),
                        fields
                    );
                    out.push_str(&format!(
                        "{indent}  {}\n",
                        fields
                            .iter()
                            .map(|k| scalar(&row[*k]))
                            .collect::<Vec<_>>()
                            .join(",")
                    ));
                }
            }
            _ => out.push_str(&format!("{indent}{key}: {}\n", scalar(v))),
        }
    }
}
pub fn print(value: Value) {
    println!("{}", encode(&value));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matches_independent_encoder_fixture() {
        let value: Value =
            serde_json::from_str(include_str!("../tests/fixtures/toon_canonical.json")).unwrap();
        assert_eq!(
            encode(&value),
            include_str!("../tests/fixtures/toon_canonical.toon")
        );
    }

    #[test]
    fn canonical_home_and_table() {
        assert_eq!(
            encode(
                &json!({"bin":"~/.local/bin/gemini-axi", "browser":"reachable", "models":["3.5 Flash Lite", "3.8 Flash", "3.1 Pro", "4 Argon"]})
            ),
            "bin: ~/.local/bin/gemini-axi\nbrowser: reachable\nmodels[4]: 3.5 Flash Lite,3.8 Flash,3.1 Pro,4 Argon"
        );
        assert_eq!(
            encode(&json!({"gems":[{"id":"abc", "name":"A, Gem"}]})),
            "gems[1]{id,name}:\n  abc,\"A, Gem\""
        );
        assert_eq!(
            encode(&json!({"my-key":[{"id":"123", "display.name":"Gem"}]})),
            "\"my-key\"[1]{id,display.name}:\n  \"123\",Gem"
        );
    }

    #[test]
    fn canonical_required_quotes_and_plain_strings() {
        assert_eq!(
            encode(
                &json!({"comma":"a,b", "colon":"a:b", "bool":"true", "number":"123", "empty":"", "plain":"Hello 世界 🦀", "count":123, "enabled":true, "none":null})
            ),
            "comma: \"a,b\"\ncolon: \"a:b\"\nbool: \"true\"\nnumber: \"123\"\nempty: \"\"\nplain: Hello 世界 🦀\ncount: 123\nenabled: true\nnone: null"
        );
        for value in [
            "false",
            "null",
            "05",
            "+1",
            "1E-6",
            "12.5e+2",
            " lead",
            "trail ",
            "-item",
            "# Heading",
            "[x]",
            "{x}",
            "a\\b",
            "say \"hi\"",
        ] {
            assert_eq!(string(value), quoted(value), "{value:?}");
        }
        for value in [
            "True",
            "NULL",
            "1.2.3",
            ".5",
            "1.",
            "1e",
            "1e+",
            "a|b",
            "a/b",
            "\u{00a0}name\u{00a0}",
        ] {
            assert_eq!(string(value), value, "{value:?}");
        }
    }

    #[test]
    fn canonical_key_rules_and_control_escapes() {
        for value in ["bin", "_name", "user.name", "field123"] {
            assert_eq!(encoded_key(value), value);
        }
        for value in ["", "123", "my-key", "a b", "ç", "a,b", "a:b"] {
            assert_eq!(encoded_key(value), quoted(value));
        }
        assert_eq!(
            encode(&json!({"response": "a\nerror: true\t\\\"ç", "n":2, "empty":[]})),
            "response: \"a\\nerror: true\\t\\\\\\\"ç\"\nn: 2\nempty: []"
        );
        assert_eq!(
            string("\0\u{0008}\u{000b}\u{001f}\r\t\n"),
            "\"\\u0000\\u0008\\u000b\\u001f\\r\\t\\n\""
        );
    }
}
