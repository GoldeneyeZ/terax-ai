use super::directory::{RecordState, TerminalDirectory, TerminalRecord};
use super::protocol::{
    build_envelope, validate_name, ControlError, ControlRequest, ControlResponse, ErrorCode,
    ResponseData, PROTOCOL_VERSION,
};
use super::{Credentials, TokenBucket};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

#[cfg(windows)]
use super::transport::windows::PipeServer;

pub const PERSIST_NAME_EVENT: &str = "terminal-control://persist-name";
const PERSIST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRecord {
    pub terminal_id: String,
    pub address_name: Option<String>,
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PersistNameRequest {
    pub request_id: String,
    pub terminal_id: String,
    pub old_name: Option<String>,
    pub new_name: String,
}

pub trait PtySink: Send + Sync {
    fn write(&self, pty_id: u32, bytes: &[u8]) -> Result<(), String>;
}

pub trait NamePersistence: Send + Sync {
    fn persist(&self, request: PersistNameRequest) -> Result<(), String>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
}

#[derive(Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

struct PendingName {
    outcome: Mutex<Option<Result<(), String>>>,
    changed: Condvar,
}

impl PendingName {
    fn new() -> Self {
        Self {
            outcome: Mutex::new(None),
            changed: Condvar::new(),
        }
    }

    fn signal(&self, outcome: Result<(), String>) -> Result<(), String> {
        let mut current = self
            .outcome
            .lock()
            .map_err(|_| "name acknowledgement lock poisoned".to_string())?;
        if current.is_some() {
            return Err("name request already acknowledged".to_string());
        }
        *current = Some(outcome);
        self.changed.notify_all();
        Ok(())
    }
}

pub struct TerminalControlState {
    directory: Mutex<TerminalDirectory>,
    credentials: Mutex<Credentials>,
    rate_limits: Mutex<HashMap<String, TokenBucket>>,
    pending_names: Mutex<HashMap<String, Arc<PendingName>>>,
    hydrated: AtomicBool,
    shutdown: AtomicBool,
    endpoint: String,
    #[cfg(windows)]
    pipe: Mutex<Option<PipeServer>>,
    pty: Arc<dyn PtySink>,
    persistence: Arc<dyn NamePersistence>,
    clock: Arc<dyn Clock>,
}

pub type ControlService = TerminalControlState;

impl TerminalControlState {
    pub fn new(
        pty: Arc<dyn PtySink>,
        persistence: Arc<dyn NamePersistence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::with_endpoint(String::new(), pty, persistence, clock)
    }

    pub fn for_app(app: tauri::AppHandle) -> Result<Self, String> {
        let endpoint = new_endpoint()?;
        Ok(Self::with_endpoint(
            endpoint,
            Arc::new(TauriPtySink { app: app.clone() }),
            Arc::new(TauriNamePersistence { app }),
            Arc::new(SystemClock),
        ))
    }

    fn with_endpoint(
        endpoint: String,
        pty: Arc<dyn PtySink>,
        persistence: Arc<dyn NamePersistence>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            directory: Mutex::new(TerminalDirectory::default()),
            credentials: Mutex::new(Credentials::default()),
            rate_limits: Mutex::new(HashMap::new()),
            pending_names: Mutex::new(HashMap::new()),
            hydrated: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            endpoint,
            #[cfg(windows)]
            pipe: Mutex::new(None),
            pty,
            persistence,
            clock,
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn sync_catalog(&self, records: Vec<CatalogRecord>) -> Result<(), ErrorCode> {
        let records = records
            .into_iter()
            .map(|record| TerminalRecord {
                terminal_id: record.terminal_id,
                address_name: record.address_name,
                private: record.private,
                state: RecordState::Persisted,
                pty_id: None,
            })
            .collect();
        self.directory
            .lock()
            .map_err(|_| ErrorCode::Internal)?
            .sync_catalog(records)?;
        self.hydrated.store(true, Ordering::Release);
        Ok(())
    }

    pub fn issue_credential(&self, terminal_id: &str) -> Result<String, ErrorCode> {
        let exists = self
            .directory
            .lock()
            .map_err(|_| ErrorCode::Internal)?
            .record(terminal_id)
            .is_some();
        if !exists {
            return Err(ErrorCode::InvalidRequest);
        }
        self.credentials
            .lock()
            .map_err(|_| ErrorCode::Internal)?
            .issue(terminal_id)
    }

    pub fn mark_live(&self, terminal_id: &str, pty_id: u32) -> Result<(), ErrorCode> {
        self.directory
            .lock()
            .map_err(|_| ErrorCode::Internal)?
            .mark_live(terminal_id, pty_id)
    }

    pub fn ack_name(&self, request_id: &str, error: Option<String>) -> Result<(), String> {
        let pending = self
            .pending_names
            .lock()
            .map_err(|_| "pending name lock poisoned".to_string())?
            .get(request_id)
            .cloned()
            .ok_or_else(|| "unknown name request".to_string())?;
        pending.signal(match error {
            Some(error) => Err(error),
            None => Ok(()),
        })
    }

    pub fn handle_request(&self, request: ControlRequest) -> ControlResponse {
        let started = Instant::now();
        let request_id = request.request_id().to_owned();
        let operation = request.operation();

        if request.version() != PROTOCOL_VERSION {
            return self.finish(
                request_id,
                operation,
                None,
                None,
                Err(ErrorCode::UnsupportedVersion),
                started,
            );
        }
        if !self.hydrated.load(Ordering::Acquire) || self.shutdown.load(Ordering::Acquire) {
            return self.finish(
                request_id,
                operation,
                None,
                None,
                Err(ErrorCode::TeraxUnavailable),
                started,
            );
        }

        let source_id = self
            .credentials
            .lock()
            .ok()
            .and_then(|credentials| credentials.authenticate(request.source_token()));
        let Some(source_id) = source_id else {
            return self.finish(
                request_id,
                operation,
                None,
                None,
                Err(ErrorCode::AuthFailed),
                started,
            );
        };

        let (result, target_id) = match request {
            ControlRequest::Name { payload, .. } => (
                self.route_name(&request_id, &source_id, &payload.name),
                None,
            ),
            ControlRequest::List { .. } => (self.route_list(), None),
            ControlRequest::Send { payload, .. } => {
                if !self.take_send_token(&source_id) {
                    (Err(ErrorCode::RateLimited), None)
                } else {
                    self.route_send(&source_id, &payload.target, &payload.message)
                }
            }
        };

        self.finish(
            request_id,
            operation,
            Some(&source_id),
            target_id.as_deref(),
            result,
            started,
        )
    }

    pub fn handle_frame(&self, request: &[u8]) -> Vec<u8> {
        let response = match serde_json::from_slice::<ControlRequest>(request) {
            Ok(request) => self.handle_request(request),
            Err(_) => ControlResponse::failure(
                "",
                ControlError::new(
                    ErrorCode::InvalidRequest,
                    error_message(ErrorCode::InvalidRequest),
                ),
            ),
        };
        serde_json::to_vec(&response).unwrap_or_else(|_| {
            br#"{"version":1,"requestId":"","ok":false,"error":{"code":"INTERNAL","message":"Internal error"}}"#.to_vec()
        })
    }

    #[cfg(windows)]
    pub fn start_pipe_server(&self, app: tauri::AppHandle) -> Result<(), String> {
        let mut pipe = self
            .pipe
            .lock()
            .map_err(|_| "terminal control pipe lock poisoned".to_string())?;
        if pipe.is_some() {
            return Ok(());
        }
        let handler_app = app;
        let server = PipeServer::spawn(self.endpoint.clone(), move |request| {
            handler_app
                .try_state::<TerminalControlState>()
                .map(|state| state.handle_frame(&request))
                .unwrap_or_else(|| {
                    serde_json::to_vec(&ControlResponse::failure(
                        "",
                        ControlError::new(
                            ErrorCode::TeraxUnavailable,
                            error_message(ErrorCode::TeraxUnavailable),
                        ),
                    ))
                    .unwrap_or_default()
                })
        })
        .map_err(|error| error.to_string())?;
        *pipe = Some(server);
        Ok(())
    }

    pub fn shutdown(&self) {
        if self.shutdown.swap(true, Ordering::AcqRel) {
            return;
        }
        #[cfg(windows)]
        if let Ok(mut pipe) = self.pipe.lock() {
            if let Some(server) = pipe.take() {
                server.shutdown();
            }
        }
        if let Ok(mut credentials) = self.credentials.lock() {
            credentials.revoke_all();
        }
        if let Ok(pending) = self.pending_names.lock() {
            for name in pending.values() {
                let _ = name.signal(Err("service shutting down".to_string()));
            }
        }
    }

    fn route_list(&self) -> Result<ResponseData, ErrorCode> {
        let names = self
            .directory
            .lock()
            .map_err(|_| ErrorCode::Internal)?
            .list_targets();
        Ok(ResponseData::List { names })
    }

    fn route_send(
        &self,
        source_id: &str,
        target_name: &str,
        message: &str,
    ) -> (Result<ResponseData, ErrorCode>, Option<String>) {
        let (source_name, target) = match self.directory.lock() {
            Ok(directory) => {
                let source_name = match directory.source_name(source_id) {
                    Ok(name) => name,
                    Err(error) => return (Err(error), None),
                };
                let target = match directory.resolve_target(target_name) {
                    Ok(target) => target,
                    Err(error) => return (Err(error), None),
                };
                (source_name, target)
            }
            Err(_) => return (Err(ErrorCode::Internal), None),
        };
        let target_id = Some(target.terminal_id.clone());
        let result = build_envelope(&source_name, message)
            .and_then(|envelope| {
                self.pty
                    .write(target.pty_id.ok_or(ErrorCode::TargetNotLive)?, &envelope)
                    .map_err(|_| ErrorCode::WriteFailed)
            })
            .map(|()| ResponseData::Send {
                target: target.address_name.unwrap_or_default(),
            });
        (result, target_id)
    }

    fn route_name(
        &self,
        request_id: &str,
        source_id: &str,
        requested_name: &str,
    ) -> Result<ResponseData, ErrorCode> {
        let canonical = validate_name(requested_name)?;
        let reservation = {
            let mut directory = self.directory.lock().map_err(|_| ErrorCode::Internal)?;
            if directory.owner(&canonical) == Some(source_id) {
                return Ok(ResponseData::Name { name: canonical });
            }
            directory.reserve_name(source_id, &canonical, request_id)?
        };
        let pending = Arc::new(PendingName::new());
        {
            let mut pending_names = self.pending_names.lock().map_err(|_| ErrorCode::Internal)?;
            if pending_names
                .insert(request_id.to_owned(), pending.clone())
                .is_some()
            {
                self.rollback_name(request_id);
                return Err(ErrorCode::InvalidRequest);
            }
        }

        let deadline = self.clock.now() + PERSIST_TIMEOUT;
        let persistence_result = self.persistence.persist(PersistNameRequest {
            request_id: reservation.request_id.clone(),
            terminal_id: reservation.terminal_id.clone(),
            old_name: reservation.old_name.clone(),
            new_name: reservation.new_name.clone(),
        });
        let outcome = if persistence_result.is_err() {
            Err(ErrorCode::PersistFailed)
        } else {
            self.wait_for_name(&pending, deadline)
        };

        if let Ok(mut pending_names) = self.pending_names.lock() {
            pending_names.remove(request_id);
        }
        match outcome {
            Ok(()) => self
                .directory
                .lock()
                .map_err(|_| ErrorCode::Internal)?
                .commit_name(request_id)
                .map(|reservation| ResponseData::Name {
                    name: reservation.new_name,
                }),
            Err(error) => {
                self.rollback_name(request_id);
                Err(error)
            }
        }
    }

    fn wait_for_name(&self, pending: &PendingName, deadline: Instant) -> Result<(), ErrorCode> {
        let mut outcome = pending.outcome.lock().map_err(|_| ErrorCode::Internal)?;
        loop {
            if let Some(outcome) = outcome.take() {
                return outcome.map_err(|_| ErrorCode::PersistFailed);
            }
            let now = self.clock.now();
            if now >= deadline {
                return Err(ErrorCode::PersistTimeout);
            }
            let (next, wait) = pending
                .changed
                .wait_timeout(outcome, deadline.saturating_duration_since(now))
                .map_err(|_| ErrorCode::Internal)?;
            outcome = next;
            if wait.timed_out() && self.clock.now() >= deadline {
                return Err(ErrorCode::PersistTimeout);
            }
        }
    }

    fn rollback_name(&self, request_id: &str) {
        if let Ok(mut directory) = self.directory.lock() {
            let _ = directory.rollback_name(request_id);
        }
    }

    fn take_send_token(&self, source_id: &str) -> bool {
        let now = self.clock.now();
        self.rate_limits
            .lock()
            .map(|mut limits| {
                limits
                    .entry(source_id.to_owned())
                    .or_insert_with(|| TokenBucket::messaging(now))
                    .take(now)
            })
            .unwrap_or(false)
    }

    fn finish(
        &self,
        request_id: String,
        operation: &str,
        source_id: Option<&str>,
        target_id: Option<&str>,
        result: Result<ResponseData, ErrorCode>,
        started: Instant,
    ) -> ControlResponse {
        let duration_micros = started.elapsed().as_micros();
        match result {
            Ok(data) => {
                log::info!(
                    "terminal control request_id={request_id} operation={operation} source_id={} target_id={} error_code=none duration_us={duration_micros}",
                    source_id.unwrap_or("none"),
                    target_id.unwrap_or("none")
                );
                ControlResponse::success(request_id, data)
            }
            Err(code) => {
                log::info!(
                    "terminal control request_id={request_id} operation={operation} source_id={} target_id={} error_code={} duration_us={duration_micros}",
                    source_id.unwrap_or("none"),
                    target_id.unwrap_or("none"),
                    code.as_str()
                );
                ControlResponse::failure(request_id, ControlError::new(code, error_message(code)))
            }
        }
    }
}

struct TauriPtySink {
    app: tauri::AppHandle,
}

impl PtySink for TauriPtySink {
    fn write(&self, pty_id: u32, bytes: &[u8]) -> Result<(), String> {
        self.app
            .try_state::<crate::modules::pty::PtyState>()
            .ok_or_else(|| "PTY state unavailable".to_string())?
            .write(pty_id, bytes)
    }
}

struct TauriNamePersistence {
    app: tauri::AppHandle,
}

impl NamePersistence for TauriNamePersistence {
    fn persist(&self, request: PersistNameRequest) -> Result<(), String> {
        self.app
            .get_webview_window("main")
            .ok_or_else(|| "main webview unavailable".to_string())?
            .emit(PERSIST_NAME_EVENT, request)
            .map_err(|error| error.to_string())
    }
}

pub fn new_endpoint() -> Result<String, String> {
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| error.to_string())?;
    let mut encoded = String::with_capacity(nonce.len() * 2);
    for byte in nonce {
        use std::fmt::Write;
        write!(&mut encoded, "{byte:02x}").map_err(|error| error.to_string())?;
    }
    Ok(format!(
        r"\\.\pipe\terax-control-{}-{encoded}",
        std::process::id()
    ))
}

fn error_message(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::TeraxUnavailable => "Terax is unavailable",
        ErrorCode::InvalidRequest => "Invalid request",
        ErrorCode::UnsupportedVersion => "Unsupported protocol version",
        ErrorCode::AuthFailed => "Authentication failed",
        ErrorCode::SourceUnnamed => "Source terminal is unnamed",
        ErrorCode::InvalidName => "Invalid terminal name",
        ErrorCode::NameInUse => "Terminal name is already in use",
        ErrorCode::TargetNotFound => "Target not found",
        ErrorCode::TargetNotLive => "Target is not live",
        ErrorCode::MessageInvalid => "Message is invalid",
        ErrorCode::MessageTooLarge => "Message is too large",
        ErrorCode::PersistFailed => "Name persistence failed",
        ErrorCode::PersistTimeout => "Name persistence timed out",
        ErrorCode::RateLimited => "Rate limit exceeded",
        ErrorCode::ServerBusy => "Server is busy",
        ErrorCode::WriteFailed => "PTY write failed",
        ErrorCode::Internal => "Internal error",
    }
}
