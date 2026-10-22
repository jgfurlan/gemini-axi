//! TOON at the process boundary. The CLI emits objects, scalar arrays and uniform tables.
use serde_json::Value;

fn string(s: &str) -> String {
    // Always quoting strings is legal TOON and prevents model output injecting structure.
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
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
    out
}
fn object(obj: &serde_json::Map<String, Value>, depth: usize, out: &mut String) {
    for (key, v) in obj {
        let indent = "  ".repeat(depth);
        let key = string(key);
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
                        .map(|k| string(k))
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
    print!("{}", encode(&value));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn preserves_untrusted_strings_and_unicode() {
        assert_eq!(
            encode(&json!({"response": "a\nerror: true\t\\\"ç", "n": 2, "empty": []})),
            "\"response\": \"a\\nerror: true\\t\\\\\\\"ç\"\n\"n\": 2\n\"empty\": []\n"
        );
        assert!(
            encode(&json!({"gems": [{"id":"1", "name":"a,b"}]}))
                .contains("[1]{\"id\",\"name\"}:\n  \"1\",\"a,b\"")
        );
    }
}
