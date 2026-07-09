use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub duration_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub ok: bool,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
    pub meta: Meta,
}

pub const CONTRACT_VERSION: &str = "1";

pub fn success<T: Serialize>(command: &str, data: T) -> serde_json::Result<String> {
    let envelope = Envelope {
        ok: true,
        version: CONTRACT_VERSION.into(),
        data: Some(data),
        error: None,
        meta: meta(command),
    };
    serde_json::to_string_pretty(&envelope).map(|mut s| {
        s.push('\n');
        s
    })
}

pub fn error(command: &str, code: &str, message: &str) -> serde_json::Result<String> {
    let envelope = Envelope::<()> {
        ok: false,
        version: CONTRACT_VERSION.into(),
        data: None,
        error: Some(ErrorBody {
            code: code.into(),
            category: None,
            message: message.into(),
            hint: None,
        }),
        meta: meta(command),
    };
    serde_json::to_string_pretty(&envelope).map(|mut s| {
        s.push('\n');
        s
    })
}

fn meta(command: &str) -> Meta {
    Meta {
        command: command.into(),
        ts: chrono::Utc::now().to_rfc3339(),
        duration_ms: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_envelope_contract_version_is_one() {
        let out = success("ping", json!({"clob":"ok"})).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["version"], "1");
        assert_eq!(v["meta"]["command"], "ping");
    }
}
