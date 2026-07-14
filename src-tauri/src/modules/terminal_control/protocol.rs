use serde::{Deserialize, Serialize};
use std::fmt;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ControlRequest {
    Name {
        version: u16,
        request_id: String,
        source_token: String,
        payload: NamePayload,
    },
    List {
        version: u16,
        request_id: String,
        source_token: String,
        payload: ListPayload,
    },
    Send {
        version: u16,
        request_id: String,
        source_token: String,
        payload: SendPayload,
    },
}

impl ControlRequest {
    pub fn version(&self) -> u16 {
        match self {
            Self::Name { version, .. }
            | Self::List { version, .. }
            | Self::Send { version, .. } => *version,
        }
    }

    pub fn request_id(&self) -> &str {
        match self {
            Self::Name { request_id, .. }
            | Self::List { request_id, .. }
            | Self::Send { request_id, .. } => request_id,
        }
    }

    pub fn source_token(&self) -> &str {
        match self {
            Self::Name { source_token, .. }
            | Self::List { source_token, .. }
            | Self::Send { source_token, .. } => source_token,
        }
    }

    pub fn operation(&self) -> &'static str {
        match self {
            Self::Name { .. } => "name",
            Self::List { .. } => "list",
            Self::Send { .. } => "send",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NamePayload {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListPayload {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SendPayload {
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    TeraxUnavailable,
    InvalidRequest,
    UnsupportedVersion,
    AuthFailed,
    SourceUnnamed,
    InvalidName,
    NameInUse,
    TargetNotFound,
    TargetNotLive,
    MessageInvalid,
    MessageTooLarge,
    PersistFailed,
    PersistTimeout,
    RateLimited,
    ServerBusy,
    WriteFailed,
    Internal,
}

impl ErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TeraxUnavailable => "TERAX_UNAVAILABLE",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::UnsupportedVersion => "UNSUPPORTED_VERSION",
            Self::AuthFailed => "AUTH_FAILED",
            Self::SourceUnnamed => "SOURCE_UNNAMED",
            Self::InvalidName => "INVALID_NAME",
            Self::NameInUse => "NAME_IN_USE",
            Self::TargetNotFound => "TARGET_NOT_FOUND",
            Self::TargetNotLive => "TARGET_NOT_LIVE",
            Self::MessageInvalid => "MESSAGE_INVALID",
            Self::MessageTooLarge => "MESSAGE_TOO_LARGE",
            Self::PersistFailed => "PERSIST_FAILED",
            Self::PersistTimeout => "PERSIST_TIMEOUT",
            Self::RateLimited => "RATE_LIMITED",
            Self::ServerBusy => "SERVER_BUSY",
            Self::WriteFailed => "WRITE_FAILED",
            Self::Internal => "INTERNAL",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl std::error::Error for ErrorCode {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlResponse {
    pub version: u16,
    pub request_id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<ResponseData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ControlError>,
}

impl ControlResponse {
    pub fn success(request_id: impl Into<String>, data: ResponseData) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn failure(request_id: impl Into<String>, error: ControlError) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            ok: false,
            data: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ResponseData {
    Name { name: String },
    List { names: Vec<String> },
    Send { target: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ControlError {
    pub code: ErrorCode,
    pub message: String,
}

impl ControlError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

pub fn validate_name(name: &str) -> Result<String, ErrorCode> {
    let canonical = name.to_ascii_lowercase();
    let bytes = canonical.as_bytes();
    let valid = matches!(bytes.first(), Some(b'a'..=b'z'))
        && bytes.len() <= 63
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');

    if valid {
        Ok(canonical)
    } else {
        Err(ErrorCode::InvalidName)
    }
}

pub fn validate_message(message: &str) -> Result<(), ErrorCode> {
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(ErrorCode::MessageTooLarge);
    }

    if message
        .chars()
        .any(|character| matches!(character as u32, 0x00..=0x08 | 0x0a..=0x1f | 0x7f..=0x9f))
    {
        return Err(ErrorCode::MessageInvalid);
    }

    Ok(())
}

pub fn build_envelope(source_name: &str, message: &str) -> Result<Vec<u8>, ErrorCode> {
    let source_name = validate_name(source_name)?;
    validate_message(message)?;

    let mut envelope = Vec::with_capacity(15 + source_name.len() + message.len());
    envelope.extend_from_slice(b"[terax from ");
    envelope.extend_from_slice(source_name.as_bytes());
    envelope.extend_from_slice(b"] ");
    envelope.extend_from_slice(message.as_bytes());
    envelope.push(b'\r');
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn requests() -> [ControlRequest; 3] {
        [
            ControlRequest::Name {
                version: PROTOCOL_VERSION,
                request_id: "req-name".into(),
                source_token: "token-a".into(),
                payload: NamePayload {
                    name: "agent-a".into(),
                },
            },
            ControlRequest::List {
                version: PROTOCOL_VERSION,
                request_id: "req-list".into(),
                source_token: "token-b".into(),
                payload: ListPayload {},
            },
            ControlRequest::Send {
                version: PROTOCOL_VERSION,
                request_id: "req-send".into(),
                source_token: "token-c".into(),
                payload: SendPayload {
                    target: "agent-b".into(),
                    message: "review commit".into(),
                },
            },
        ]
    }

    #[test]
    fn request_variants_use_exact_wire_fields_and_round_trip() {
        let expected = [
            json!({
                "operation": "name",
                "version": 1,
                "requestId": "req-name",
                "sourceToken": "token-a",
                "payload": { "name": "agent-a" }
            }),
            json!({
                "operation": "list",
                "version": 1,
                "requestId": "req-list",
                "sourceToken": "token-b",
                "payload": {}
            }),
            json!({
                "operation": "send",
                "version": 1,
                "requestId": "req-send",
                "sourceToken": "token-c",
                "payload": { "target": "agent-b", "message": "review commit" }
            }),
        ];

        for (request, expected) in requests().into_iter().zip(expected) {
            assert_eq!(serde_json::to_value(&request).unwrap(), expected);
            let decoded: ControlRequest = serde_json::from_value(expected).unwrap();
            assert_eq!(decoded, request);
        }
    }

    #[test]
    fn common_request_accessors_do_not_depend_on_operation() {
        let expected = [
            ("name", "req-name", "token-a"),
            ("list", "req-list", "token-b"),
            ("send", "req-send", "token-c"),
        ];

        for (request, (operation, request_id, token)) in requests().iter().zip(expected) {
            assert_eq!(request.version(), PROTOCOL_VERSION);
            assert_eq!(request.operation(), operation);
            assert_eq!(request.request_id(), request_id);
            assert_eq!(request.source_token(), token);
        }
    }

    #[test]
    fn response_data_variants_use_exact_untagged_shapes_and_round_trip() {
        let cases = [
            (
                ResponseData::Name {
                    name: "agent-a".into(),
                },
                json!({ "name": "agent-a" }),
            ),
            (
                ResponseData::List {
                    names: vec!["agent-a".into(), "agent-b".into()],
                },
                json!({ "names": ["agent-a", "agent-b"] }),
            ),
            (
                ResponseData::Send {
                    target: "agent-b".into(),
                },
                json!({ "target": "agent-b" }),
            ),
        ];

        for (data, expected) in cases {
            assert_eq!(serde_json::to_value(&data).unwrap(), expected);
            let decoded: ResponseData = serde_json::from_value(expected).unwrap();
            assert_eq!(decoded, data);
        }
    }

    #[test]
    fn responses_omit_the_inapplicable_success_or_error_field() {
        let success = ControlResponse::success(
            "req-1",
            ResponseData::Send {
                target: "agent-b".into(),
            },
        );
        assert_eq!(
            serde_json::to_value(success).unwrap(),
            json!({
                "version": 1,
                "requestId": "req-1",
                "ok": true,
                "data": { "target": "agent-b" }
            })
        );

        let failure = ControlResponse::failure(
            "req-2",
            ControlError::new(ErrorCode::TargetNotLive, "Target is not live"),
        );
        assert_eq!(
            serde_json::to_value(failure).unwrap(),
            json!({
                "version": 1,
                "requestId": "req-2",
                "ok": false,
                "error": {
                    "code": "TARGET_NOT_LIVE",
                    "message": "Target is not live"
                }
            })
        );
    }

    #[test]
    fn every_error_code_has_the_stable_screaming_snake_case_value() {
        let cases = [
            (ErrorCode::TeraxUnavailable, "TERAX_UNAVAILABLE"),
            (ErrorCode::InvalidRequest, "INVALID_REQUEST"),
            (ErrorCode::UnsupportedVersion, "UNSUPPORTED_VERSION"),
            (ErrorCode::AuthFailed, "AUTH_FAILED"),
            (ErrorCode::SourceUnnamed, "SOURCE_UNNAMED"),
            (ErrorCode::InvalidName, "INVALID_NAME"),
            (ErrorCode::NameInUse, "NAME_IN_USE"),
            (ErrorCode::TargetNotFound, "TARGET_NOT_FOUND"),
            (ErrorCode::TargetNotLive, "TARGET_NOT_LIVE"),
            (ErrorCode::MessageInvalid, "MESSAGE_INVALID"),
            (ErrorCode::MessageTooLarge, "MESSAGE_TOO_LARGE"),
            (ErrorCode::PersistFailed, "PERSIST_FAILED"),
            (ErrorCode::PersistTimeout, "PERSIST_TIMEOUT"),
            (ErrorCode::RateLimited, "RATE_LIMITED"),
            (ErrorCode::ServerBusy, "SERVER_BUSY"),
            (ErrorCode::WriteFailed, "WRITE_FAILED"),
            (ErrorCode::Internal, "INTERNAL"),
        ];

        for (code, expected) in cases {
            assert_eq!(code.as_str(), expected);
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                Value::String(expected.into())
            );
            assert_eq!(
                serde_json::from_value::<ErrorCode>(Value::String(expected.into())).unwrap(),
                code
            );
        }
    }

    #[test]
    fn name_validation_ascii_lowercases_and_enforces_the_wire_grammar() {
        assert_eq!(validate_name("Agent-42"), Ok("agent-42".into()));
        assert_eq!(validate_name("a"), Ok("a".into()));
        assert_eq!(
            validate_name(&format!("a{}", "9".repeat(62))),
            Ok(format!("a{}", "9".repeat(62)))
        );

        for invalid in ["", "9agent", "agent_1", "agent--?", "agent name", "éclair"] {
            assert_eq!(validate_name(invalid), Err(ErrorCode::InvalidName));
        }
        assert_eq!(
            validate_name(&format!("a{}", "9".repeat(63))),
            Err(ErrorCode::InvalidName)
        );
    }

    #[test]
    fn message_validation_rejects_controls_and_oversize_utf8_but_allows_tab() {
        assert_eq!(validate_message("one\ttwo"), Ok(()));
        assert_eq!(validate_message(&"a".repeat(MAX_MESSAGE_BYTES)), Ok(()));

        for invalid in ["a\nb", "a\rb", "\0", "\u{1b}[31m", "\u{7f}", "\u{85}"] {
            assert_eq!(validate_message(invalid), Err(ErrorCode::MessageInvalid));
        }
        assert_eq!(
            validate_message(&"a".repeat(MAX_MESSAGE_BYTES + 1)),
            Err(ErrorCode::MessageTooLarge)
        );
        assert_eq!(
            validate_message(&"é".repeat(MAX_MESSAGE_BYTES / 2 + 1)),
            Err(ErrorCode::MessageTooLarge)
        );
    }

    #[test]
    fn send_builds_one_plain_envelope() {
        let bytes = build_envelope("agent-a", "review commit").unwrap();
        assert_eq!(bytes, b"[terax from agent-a] review commit\r");
        assert_eq!(bytes.iter().filter(|byte| **byte == b'\r').count(), 1);
    }
}
