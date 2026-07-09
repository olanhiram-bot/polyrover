use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

pub fn scalar_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

pub fn string_or_number<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(scalar_to_string(&value))
}

pub fn bool_or_false<'de, D>(deserializer: D) -> std::result::Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(matches!(
        scalar_to_string(&value).to_ascii_lowercase().as_str(),
        "true" | "1"
    ))
}

pub fn float_or_zero<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(scalar_to_string(&value).parse().unwrap_or(0.0))
}

pub fn int_or_zero<'de, D>(deserializer: D) -> std::result::Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(scalar_to_string(&value).parse().unwrap_or(0))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StringOrArray(pub Vec<String>);

impl<'de> Deserialize<'de> for StringOrArray {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        Ok(Self(flatten_strings(value).map_err(de::Error::custom)?))
    }
}

impl Serialize for StringOrArray {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0.serialize(serializer)
    }
}

fn flatten_strings(value: Value) -> std::result::Result<Vec<String>, serde_json::Error> {
    match value {
        Value::Null => Ok(vec![]),
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(vec![]);
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                return flatten_strings(serde_json::from_str(trimmed)?);
            }
            Ok(vec![s])
        }
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                out.extend(flatten_strings(item)?);
            }
            Ok(out)
        }
        _ => Ok(vec![]),
    }
}

pub fn first_non_empty<'a>(values: impl IntoIterator<Item = &'a str>) -> &'a str {
    values.into_iter().find(|v| !v.is_empty()).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Row {
        values: StringOrArray,
    }

    #[test]
    fn string_or_array_accepts_gamma_shapes() {
        for (json, want) in [
            (r#"{"values":"YES"}"#, vec!["YES"]),
            (r#"{"values":"[\"YES\",\"NO\"]"}"#, vec!["YES", "NO"]),
            (r#"{"values":[["YES"],["NO"]]}"#, vec!["YES", "NO"]),
            (r#"{"values":null}"#, vec![]),
        ] {
            let row: Row = serde_json::from_str(json).unwrap();
            assert_eq!(row.values.0, want);
        }
    }

    #[test]
    fn scalar_preserves_large_numbers_as_text() {
        let v: Value = serde_json::from_str("123456789012345678901234567890").unwrap();
        assert_eq!(scalar_to_string(&v), "123456789012345678901234567890");
    }
}
