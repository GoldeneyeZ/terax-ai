use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};
use terax_lib::modules::terminal_control::{
    CatalogRecord, Clock, ControlRequest, ControlService, ErrorCode, ListPayload, NamePayload,
    NamePersistence, PersistNameRequest, PtySink, ResponseData, SendPayload, PROTOCOL_VERSION,
};

#[derive(Default)]
struct FakePty {
    writes: Mutex<Vec<(u32, Vec<u8>)>>,
    fail: AtomicBool,
}

impl PtySink for FakePty {
    fn write(&self, pty_id: u32, bytes: &[u8]) -> Result<(), String> {
        if self.fail.load(Ordering::Acquire) {
            return Err("broken pipe".into());
        }
        self.writes.lock().unwrap().push((pty_id, bytes.to_vec()));
        Ok(())
    }
}

struct FakeClock {
    start: Instant,
    elapsed_millis: AtomicU64,
}

impl Default for FakeClock {
    fn default() -> Self {
        Self {
            start: Instant::now(),
            elapsed_millis: AtomicU64::new(0),
        }
    }
}

impl FakeClock {
    fn advance(&self, duration: Duration) {
        self.elapsed_millis
            .fetch_add(duration.as_millis() as u64, Ordering::AcqRel);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.start + Duration::from_millis(self.elapsed_millis.load(Ordering::Acquire))
    }
}

#[derive(Default)]
struct FakePersistence {
    requests: Mutex<Vec<PersistNameRequest>>,
    requested: Condvar,
    emit_error: Mutex<Option<String>>,
    advance_clock: Mutex<Option<(Arc<FakeClock>, Duration)>>,
}

impl FakePersistence {
    fn wait_for_request(&self) -> PersistNameRequest {
        let requests = self.requests.lock().unwrap();
        let (requests, wait) = self
            .requested
            .wait_timeout_while(requests, Duration::from_secs(1), |requests| {
                requests.is_empty()
            })
            .unwrap();
        assert!(!wait.timed_out(), "persistence request was not emitted");
        requests.last().unwrap().clone()
    }
}

impl NamePersistence for FakePersistence {
    fn persist(&self, request: PersistNameRequest) -> Result<(), String> {
        if let Some(error) = self.emit_error.lock().unwrap().clone() {
            return Err(error);
        }
        self.requests.lock().unwrap().push(request);
        self.requested.notify_all();
        if let Some((clock, duration)) = self.advance_clock.lock().unwrap().clone() {
            clock.advance(duration);
        }
        Ok(())
    }
}

struct Fixture {
    service: Arc<ControlService>,
    pty: Arc<FakePty>,
    persistence: Arc<FakePersistence>,
    clock: Arc<FakeClock>,
    source_token: String,
}

impl Fixture {
    fn new() -> Self {
        let pty = Arc::new(FakePty::default());
        let persistence = Arc::new(FakePersistence::default());
        let clock = Arc::new(FakeClock::default());
        let service = Arc::new(ControlService::new(
            pty.clone(),
            persistence.clone(),
            clock.clone(),
        ));
        service
            .sync_catalog(vec![
                catalog("source", Some("agent-a"), false),
                catalog("target", Some("agent-b"), false),
            ])
            .unwrap();
        service.mark_live("source", 1).unwrap();
        service.mark_live("target", 2).unwrap();
        let source_token = service.issue_credential("source").unwrap();
        Self {
            service,
            pty,
            persistence,
            clock,
            source_token,
        }
    }

    fn send(&self, request_id: impl Into<String>, target: &str, message: &str) -> ResponseData {
        let response = self.service.handle_request(ControlRequest::Send {
            version: PROTOCOL_VERSION,
            request_id: request_id.into(),
            source_token: self.source_token.clone(),
            payload: SendPayload {
                target: target.into(),
                message: message.into(),
            },
        });
        assert!(response.ok, "unexpected send error: {:?}", response.error);
        response.data.unwrap()
    }
}

fn catalog(terminal_id: &str, address_name: Option<&str>, private: bool) -> CatalogRecord {
    CatalogRecord {
        terminal_id: terminal_id.into(),
        address_name: address_name.map(str::to_owned),
        private,
    }
}

fn error_code(response: terax_lib::modules::terminal_control::ControlResponse) -> ErrorCode {
    assert!(!response.ok);
    response.error.unwrap().code
}

#[test]
fn list_masks_private_unnamed_inactive_and_conflicted_records() {
    let fixture = Fixture::new();
    fixture
        .service
        .sync_catalog(vec![
            catalog("source", Some("agent-a"), false),
            catalog("target", Some("agent-b"), false),
            catalog("private", Some("secret"), true),
            catalog("unnamed", None, false),
            catalog("inactive", Some("sleeping"), false),
            catalog("conflict-a", Some("duplicate"), false),
            catalog("conflict-b", Some("duplicate"), false),
        ])
        .unwrap();
    fixture.service.mark_live("private", 3).unwrap();
    fixture.service.mark_live("unnamed", 4).unwrap();
    fixture.service.mark_live("conflict-a", 5).unwrap();
    fixture.service.mark_live("conflict-b", 6).unwrap();

    let response = fixture.service.handle_request(ControlRequest::List {
        version: PROTOCOL_VERSION,
        request_id: "list-1".into(),
        source_token: fixture.source_token.clone(),
        payload: ListPayload {},
    });

    assert_eq!(
        response.data,
        Some(ResponseData::List {
            names: vec!["agent-a".into(), "agent-b".into()],
        })
    );
}

#[test]
fn authenticated_send_writes_one_exact_envelope_to_the_resolved_pty() {
    let fixture = Fixture::new();

    assert_eq!(
        fixture.send("send-1", "Agent-B", "review commit"),
        ResponseData::Send {
            target: "agent-b".into(),
        }
    );
    assert_eq!(
        *fixture.pty.writes.lock().unwrap(),
        vec![(2, b"[terax from agent-a] review commit\r".to_vec())]
    );
}

#[test]
fn unnamed_source_cannot_send() {
    let fixture = Fixture::new();
    fixture
        .service
        .sync_catalog(vec![
            catalog("source", Some("agent-a"), false),
            catalog("target", Some("agent-b"), false),
            catalog("unnamed", None, true),
        ])
        .unwrap();
    fixture.service.mark_live("unnamed", 3).unwrap();
    let token = fixture.service.issue_credential("unnamed").unwrap();

    let response = fixture.service.handle_request(ControlRequest::Send {
        version: PROTOCOL_VERSION,
        request_id: "send-unnamed".into(),
        source_token: token,
        payload: SendPayload {
            target: "agent-b".into(),
            message: "hello".into(),
        },
    });

    assert_eq!(error_code(response), ErrorCode::SourceUnnamed);
    assert!(fixture.pty.writes.lock().unwrap().is_empty());
}

#[test]
fn backend_write_failure_is_never_reported_as_success() {
    let fixture = Fixture::new();
    fixture.pty.fail.store(true, Ordering::Release);

    let response = fixture.service.handle_request(ControlRequest::Send {
        version: PROTOCOL_VERSION,
        request_id: "send-failed".into(),
        source_token: fixture.source_token.clone(),
        payload: SendPayload {
            target: "agent-b".into(),
            message: "hello".into(),
        },
    });

    assert_eq!(error_code(response), ErrorCode::WriteFailed);
}

#[test]
fn send_rate_limit_is_applied_per_authenticated_source() {
    let fixture = Fixture::new();

    for index in 0..40 {
        fixture.send(format!("send-{index}"), "agent-b", "hello");
    }
    let response = fixture.service.handle_request(ControlRequest::Send {
        version: PROTOCOL_VERSION,
        request_id: "send-41".into(),
        source_token: fixture.source_token.clone(),
        payload: SendPayload {
            target: "agent-b".into(),
            message: "hello".into(),
        },
    });

    assert_eq!(error_code(response), ErrorCode::RateLimited);
    assert_eq!(fixture.pty.writes.lock().unwrap().len(), 40);
}

#[test]
fn name_commits_only_after_frontend_acknowledges_persistence() {
    let fixture = Fixture::new();
    let service = fixture.service.clone();
    let token = fixture.source_token.clone();
    let request = thread::spawn(move || {
        service.handle_request(ControlRequest::Name {
            version: PROTOCOL_VERSION,
            request_id: "name-commit".into(),
            source_token: token,
            payload: NamePayload {
                name: "Agent-Renamed".into(),
            },
        })
    });

    assert_eq!(
        fixture.persistence.wait_for_request(),
        PersistNameRequest {
            request_id: "name-commit".into(),
            terminal_id: "source".into(),
            old_name: Some("agent-a".into()),
            new_name: "agent-renamed".into(),
        }
    );
    fixture.service.ack_name("name-commit", None).unwrap();
    assert_eq!(
        request.join().unwrap().data,
        Some(ResponseData::Name {
            name: "agent-renamed".into(),
        })
    );

    fixture.send("send-renamed", "agent-b", "hello");
    assert_eq!(
        fixture.pty.writes.lock().unwrap().last().unwrap().1,
        b"[terax from agent-renamed] hello\r"
    );
}

#[test]
fn name_failure_rolls_back_and_retains_the_old_committed_name() {
    let fixture = Fixture::new();
    let service = fixture.service.clone();
    let token = fixture.source_token.clone();
    let request = thread::spawn(move || {
        service.handle_request(ControlRequest::Name {
            version: PROTOCOL_VERSION,
            request_id: "name-failed".into(),
            source_token: token,
            payload: NamePayload {
                name: "agent-renamed".into(),
            },
        })
    });

    fixture.persistence.wait_for_request();
    fixture
        .service
        .ack_name("name-failed", Some("save failed".into()))
        .unwrap();
    assert_eq!(
        error_code(request.join().unwrap()),
        ErrorCode::PersistFailed
    );

    fixture.send("send-after-failure", "agent-b", "hello");
    assert_eq!(
        fixture.pty.writes.lock().unwrap().last().unwrap().1,
        b"[terax from agent-a] hello\r"
    );
}

#[test]
fn name_times_out_after_five_seconds_and_releases_the_reservation() {
    let fixture = Fixture::new();
    *fixture.persistence.advance_clock.lock().unwrap() =
        Some((fixture.clock.clone(), Duration::from_secs(5)));

    let response = fixture.service.handle_request(ControlRequest::Name {
        version: PROTOCOL_VERSION,
        request_id: "name-timeout".into(),
        source_token: fixture.source_token.clone(),
        payload: NamePayload {
            name: "agent-renamed".into(),
        },
    });

    assert_eq!(error_code(response), ErrorCode::PersistTimeout);
    fixture.send("send-after-timeout", "agent-b", "hello");
    assert_eq!(
        fixture.pty.writes.lock().unwrap().last().unwrap().1,
        b"[terax from agent-a] hello\r"
    );
}

#[test]
fn version_and_hydration_checks_precede_authentication() {
    let pty = Arc::new(FakePty::default());
    let persistence = Arc::new(FakePersistence::default());
    let clock = Arc::new(FakeClock::default());
    let service = ControlService::new(pty, persistence, clock);

    let unsupported = service.handle_request(ControlRequest::List {
        version: PROTOCOL_VERSION + 1,
        request_id: "unsupported".into(),
        source_token: "forged".into(),
        payload: ListPayload {},
    });
    assert_eq!(error_code(unsupported), ErrorCode::UnsupportedVersion);

    let unhydrated = service.handle_request(ControlRequest::List {
        version: PROTOCOL_VERSION,
        request_id: "unhydrated".into(),
        source_token: "forged".into(),
        payload: ListPayload {},
    });
    assert_eq!(error_code(unhydrated), ErrorCode::TeraxUnavailable);
}
