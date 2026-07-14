use terax_lib::modules::terminal_control::cli::{
    build_request, exit_code, help_text, parse, render_response, CliCommand,
};
use terax_lib::modules::terminal_control::{
    ControlError, ControlRequest, ControlResponse, ErrorCode, ResponseData, PROTOCOL_VERSION,
};

#[cfg(windows)]
use std::process::Command;
#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(windows)]
use terax_lib::modules::terminal_control::transport::windows::PipeServer;

#[test]
fn parses_all_commands_and_requires_exact_arguments() {
    assert_eq!(
        parse(["name", "Agent-A"]),
        Ok(CliCommand::Name {
            name: "Agent-A".into(),
            json: false,
        })
    );
    assert_eq!(
        parse(["list", "--json"]),
        Ok(CliCommand::List { json: true })
    );
    assert_eq!(
        parse(["send", "agent-b", "review commit"]),
        Ok(CliCommand::Send {
            target: "agent-b".into(),
            message: "review commit".into(),
            json: false,
        })
    );

    assert!(parse(["send", "agent-b", "two", "args"]).is_err());
    assert!(parse(["list", "extra"]).is_err());
    assert!(parse(["name"]).is_err());
}

#[test]
fn help_is_handled_without_credentials() {
    assert_eq!(parse(["--help"]), Ok(CliCommand::Help));
    let help = help_text();
    assert!(help.contains("teraxctl name <name> [--json]"));
    assert!(help.contains("teraxctl list [--json]"));
    assert!(help.contains("teraxctl send <target> <message> [--json]"));
}

#[test]
fn invalid_usage_maps_to_exit_two() {
    let error = parse(["send", "target-only"]).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidRequest);
    assert_eq!(exit_code(error.code), 2);
}

#[test]
fn missing_environment_returns_local_auth_failed() {
    let command = CliCommand::List { json: false };
    let missing_endpoint = build_request(&command, None, Some("secret"), "req-1").unwrap_err();
    let missing_token =
        build_request(&command, Some(r"\\.\pipe\terax"), None, "req-2").unwrap_err();

    assert_eq!(missing_endpoint.code, ErrorCode::AuthFailed);
    assert_eq!(missing_token.code, ErrorCode::AuthFailed);
    assert_eq!(exit_code(missing_token.code), 3);
}

#[test]
fn builds_typed_request_without_accepting_source_identity() {
    let (endpoint, request) = build_request(
        &CliCommand::Send {
            target: "agent-b".into(),
            message: "review commit".into(),
            json: false,
        },
        Some(r"\\.\pipe\terax-control"),
        Some("capability-token"),
        "00112233445566778899aabbccddeeff",
    )
    .unwrap();

    assert_eq!(endpoint, r"\\.\pipe\terax-control");
    assert_eq!(request.version(), PROTOCOL_VERSION);
    assert_eq!(request.request_id(), "00112233445566778899aabbccddeeff");
    assert_eq!(request.source_token(), "capability-token");
    assert!(matches!(
        request,
        ControlRequest::Send { payload, .. }
            if payload.target == "agent-b" && payload.message == "review commit"
    ));
}

#[test]
fn renders_sorted_human_list_and_json_without_token() {
    let response = ControlResponse::success(
        "req-list",
        ResponseData::List {
            names: vec!["zeta".into(), "alpha".into(), "mid".into()],
        },
    );

    assert_eq!(
        render_response(&response, false).unwrap(),
        "alpha\nmid\nzeta"
    );
    let json = render_response(&response, true).unwrap();
    assert!(!json.contains("capability-token"));
    assert!(json.contains("req-list"));
    assert!(json.contains("alpha"));
}

#[test]
fn maps_all_typed_errors_to_stable_exit_codes() {
    let cases = [
        (ErrorCode::Internal, 1),
        (ErrorCode::InvalidRequest, 2),
        (ErrorCode::UnsupportedVersion, 2),
        (ErrorCode::InvalidName, 2),
        (ErrorCode::MessageInvalid, 2),
        (ErrorCode::MessageTooLarge, 2),
        (ErrorCode::TeraxUnavailable, 3),
        (ErrorCode::AuthFailed, 3),
        (ErrorCode::SourceUnnamed, 3),
        (ErrorCode::NameInUse, 4),
        (ErrorCode::PersistFailed, 4),
        (ErrorCode::PersistTimeout, 4),
        (ErrorCode::TargetNotFound, 5),
        (ErrorCode::TargetNotLive, 5),
        (ErrorCode::RateLimited, 6),
        (ErrorCode::ServerBusy, 6),
        (ErrorCode::WriteFailed, 7),
    ];

    for (code, expected) in cases {
        assert_eq!(exit_code(code), expected, "{}", code.as_str());
    }
}

#[cfg(windows)]
fn pipe_endpoint(label: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!(r"\\.\pipe\teraxctl-{label}-{}-{nonce}", std::process::id())
}

#[cfg(windows)]
#[test]
fn binary_help_and_missing_environment_use_stable_exits() {
    let binary = env!("CARGO_BIN_EXE_teraxctl");
    let help = Command::new(binary)
        .arg("--help")
        .env_remove("TERAX_IPC_ENDPOINT")
        .env_remove("TERAX_IPC_TOKEN")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("teraxctl send"));

    let missing = Command::new(binary)
        .arg("list")
        .env_remove("TERAX_IPC_ENDPOINT")
        .env_remove("TERAX_IPC_TOKEN")
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&missing.stderr).contains("AUTH_FAILED"));
}

#[cfg(windows)]
#[test]
fn binary_calls_pipe_once_and_sorts_human_list() {
    let endpoint = pipe_endpoint("list");
    let server = PipeServer::spawn(endpoint.clone(), |frame| {
        let request: ControlRequest = serde_json::from_slice(&frame).unwrap();
        assert_eq!(request.source_token(), "secret-list");
        serde_json::to_vec(&ControlResponse::success(
            request.request_id(),
            ResponseData::List {
                names: vec!["zeta".into(), "alpha".into()],
            },
        ))
        .unwrap()
    })
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_teraxctl"))
        .arg("list")
        .env("TERAX_IPC_ENDPOINT", &endpoint)
        .env("TERAX_IPC_TOKEN", "secret-list")
        .output()
        .unwrap();
    server.shutdown();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "alpha\nzeta"
    );
    assert!(output.stderr.is_empty());
}

#[cfg(windows)]
#[test]
fn binary_json_error_is_valid_and_never_echoes_token() {
    let endpoint = pipe_endpoint("error");
    let server = PipeServer::spawn(endpoint.clone(), |frame| {
        let request: ControlRequest = serde_json::from_slice(&frame).unwrap();
        serde_json::to_vec(&ControlResponse::failure(
            request.request_id(),
            ControlError::new(ErrorCode::AuthFailed, "denied"),
        ))
        .unwrap()
    })
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_teraxctl"))
        .args(["list", "--json"])
        .env("TERAX_IPC_ENDPOINT", &endpoint)
        .env("TERAX_IPC_TOKEN", "secret-json-token")
        .output()
        .unwrap();
    server.shutdown();

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let response: ControlResponse = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(response.error.unwrap().code, ErrorCode::AuthFailed);
    assert!(!stderr.contains("secret-json-token"));
}
