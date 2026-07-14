use std::env;
use std::fmt::{self, Write as _};
#[cfg(windows)]
use std::time::Duration;

use super::{
    ControlRequest, ControlResponse, ErrorCode, ListPayload, NamePayload, ResponseData,
    SendPayload, PROTOCOL_VERSION,
};

pub const ENDPOINT_ENV: &str = "TERAX_IPC_ENDPOINT";
pub const TOKEN_ENV: &str = "TERAX_IPC_TOKEN";

const HELP: &str = "Usage:\n  teraxctl name <name> [--json]\n  teraxctl list [--json]\n  teraxctl send <target> <message> [--json]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliCommand {
    Help,
    Name {
        name: String,
        json: bool,
    },
    List {
        json: bool,
    },
    Send {
        target: String,
        message: String,
        json: bool,
    },
}

impl CliCommand {
    fn json(&self) -> bool {
        match self {
            Self::Help => false,
            Self::Name { json, .. } | Self::List { json } | Self::Send { json, .. } => *json,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliFailure {
    pub code: ErrorCode,
    pub message: String,
    rendered: bool,
}

impl CliFailure {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            rendered: false,
        }
    }

    fn rendered(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            rendered: true,
        }
    }

    fn usage(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }
}

impl fmt::Display for CliFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.rendered {
            formatter.write_str(&self.message)
        } else {
            write!(formatter, "{}: {}", self.code.as_str(), self.message)
        }
    }
}

impl std::error::Error for CliFailure {}

pub fn help_text() -> &'static str {
    HELP
}

pub fn parse<I, S>(args: I) -> Result<CliCommand, CliFailure>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect::<Vec<_>>();

    if matches!(args.as_slice(), [flag] if flag == "--help" || flag == "-h" || flag == "help") {
        return Ok(CliCommand::Help);
    }

    let json = matches!(args.last(), Some(flag) if flag == "--json");
    if json {
        args.pop();
    }
    if args.iter().any(|argument| argument == "--json") {
        return Err(CliFailure::usage("--json must be the final argument"));
    }

    match args.as_slice() {
        [command, name] if command == "name" => Ok(CliCommand::Name {
            name: name.clone(),
            json,
        }),
        [command] if command == "list" => Ok(CliCommand::List { json }),
        [command, target, message] if command == "send" => Ok(CliCommand::Send {
            target: target.clone(),
            message: message.clone(),
            json,
        }),
        _ => Err(CliFailure::usage(HELP)),
    }
}

pub fn build_request(
    command: &CliCommand,
    endpoint: Option<&str>,
    token: Option<&str>,
    request_id: &str,
) -> Result<(String, ControlRequest), CliFailure> {
    let endpoint = required_environment(endpoint, ENDPOINT_ENV)?;
    let source_token = required_environment(token, TOKEN_ENV)?.to_owned();
    let request_id = request_id.to_owned();

    let request = match command {
        CliCommand::Help => {
            return Err(CliFailure::usage("help does not create a control request"));
        }
        CliCommand::Name { name, .. } => ControlRequest::Name {
            version: PROTOCOL_VERSION,
            request_id,
            source_token,
            payload: NamePayload { name: name.clone() },
        },
        CliCommand::List { .. } => ControlRequest::List {
            version: PROTOCOL_VERSION,
            request_id,
            source_token,
            payload: ListPayload {},
        },
        CliCommand::Send {
            target, message, ..
        } => ControlRequest::Send {
            version: PROTOCOL_VERSION,
            request_id,
            source_token,
            payload: SendPayload {
                target: target.clone(),
                message: message.clone(),
            },
        },
    };

    Ok((endpoint.to_owned(), request))
}

fn required_environment<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, CliFailure> {
    value.filter(|value| !value.is_empty()).ok_or_else(|| {
        CliFailure::new(
            ErrorCode::AuthFailed,
            format!("missing required environment variable {name}"),
        )
    })
}

pub const fn exit_code(code: ErrorCode) -> i32 {
    match code {
        ErrorCode::Internal => 1,
        ErrorCode::InvalidRequest
        | ErrorCode::UnsupportedVersion
        | ErrorCode::InvalidName
        | ErrorCode::MessageInvalid
        | ErrorCode::MessageTooLarge => 2,
        ErrorCode::TeraxUnavailable | ErrorCode::AuthFailed | ErrorCode::SourceUnnamed => 3,
        ErrorCode::NameInUse | ErrorCode::PersistFailed | ErrorCode::PersistTimeout => 4,
        ErrorCode::TargetNotFound | ErrorCode::TargetNotLive => 5,
        ErrorCode::RateLimited | ErrorCode::ServerBusy => 6,
        ErrorCode::WriteFailed => 7,
    }
}

pub fn render_response(response: &ControlResponse, json: bool) -> Result<String, CliFailure> {
    if json {
        return serde_json::to_string(response).map_err(|error| {
            CliFailure::new(
                ErrorCode::Internal,
                format!("failed to encode response: {error}"),
            )
        });
    }

    if let Some(error) = &response.error {
        return Ok(format!("{}: {}", error.code.as_str(), error.message));
    }

    match response.data.as_ref() {
        Some(ResponseData::Name { name }) => Ok(name.clone()),
        Some(ResponseData::List { names }) => {
            let mut names = names.clone();
            names.sort_unstable();
            Ok(names.join("\n"))
        }
        Some(ResponseData::Send { target }) => Ok(target.clone()),
        None => Err(CliFailure::new(
            ErrorCode::Internal,
            "response contains neither data nor error",
        )),
    }
}

pub fn run<I, S>(args: I) -> i32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    match run_inner(args) {
        Ok(output) => {
            println!("{output}");
            0
        }
        Err(error) => {
            eprintln!("{error}");
            exit_code(error.code)
        }
    }
}

fn run_inner<I, S>(args: I) -> Result<String, CliFailure>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let command = parse(args)?;
    if command == CliCommand::Help {
        return Ok(HELP.to_owned());
    }

    let request_id = random_request_id()?;
    let endpoint = env::var(ENDPOINT_ENV).ok();
    let token = env::var(TOKEN_ENV).ok();
    let (endpoint, request) =
        build_request(&command, endpoint.as_deref(), token.as_deref(), &request_id)?;
    let request = serde_json::to_vec(&request).map_err(|error| {
        CliFailure::new(
            ErrorCode::Internal,
            format!("failed to encode request: {error}"),
        )
    })?;
    let response = call_server(&endpoint, &request)?;
    if response.version != PROTOCOL_VERSION {
        return Err(CliFailure::new(
            ErrorCode::UnsupportedVersion,
            format!("unsupported response version {}", response.version),
        ));
    }
    if response.request_id != request_id {
        return Err(CliFailure::new(
            ErrorCode::InvalidRequest,
            "response request ID does not match request",
        ));
    }

    let output = render_response(&response, command.json())?;
    if response.ok {
        Ok(output)
    } else {
        let code = response
            .error
            .as_ref()
            .map_or(ErrorCode::Internal, |error| error.code);
        Err(CliFailure::rendered(code, output))
    }
}

fn random_request_id() -> Result<String, CliFailure> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        CliFailure::new(
            ErrorCode::Internal,
            format!("failed to generate request ID: {error}"),
        )
    })?;
    let mut encoded = String::with_capacity(32);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok(encoded)
}

#[cfg(windows)]
fn call_server(endpoint: &str, request: &[u8]) -> Result<ControlResponse, CliFailure> {
    let response = super::transport::windows::call(endpoint, request, Duration::from_secs(2))
        .map_err(|error| CliFailure::new(error.code(), error.to_string()))?;
    serde_json::from_slice(&response).map_err(|error| {
        CliFailure::new(
            ErrorCode::InvalidRequest,
            format!("failed to decode response: {error}"),
        )
    })
}

#[cfg(not(windows))]
fn call_server(_endpoint: &str, _request: &[u8]) -> Result<ControlResponse, CliFailure> {
    Err(CliFailure::new(
        ErrorCode::TeraxUnavailable,
        "teraxctl is available only in Windows-native Terax terminals",
    ))
}
