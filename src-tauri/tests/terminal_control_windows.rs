#![cfg(windows)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Barrier, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use terax_lib::modules::terminal_control::cli::{build_request, CliCommand};
use terax_lib::modules::terminal_control::transport::windows::{call, PipeServer};
use terax_lib::modules::terminal_control::{
    CatalogRecord, Clock, ControlResponse, ControlService, ErrorCode, NamePersistence,
    PersistNameRequest, PtySink,
};

#[derive(Default)]
struct GateState {
    blocked: bool,
    entered: usize,
    active: usize,
    max_active: usize,
    writes: Vec<(u32, Vec<u8>)>,
}

#[derive(Default)]
struct GatePty {
    state: Mutex<GateState>,
    changed: Condvar,
}

impl GatePty {
    fn blocked() -> Self {
        Self {
            state: Mutex::new(GateState {
                blocked: true,
                ..GateState::default()
            }),
            changed: Condvar::new(),
        }
    }

    fn wait_for_entries(&self, expected: usize) {
        let state = self.state.lock().unwrap();
        let (state, wait) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(2), |state| {
                state.entered < expected
            })
            .unwrap();
        assert!(!wait.timed_out(), "PTY write {expected} did not start");
        assert_eq!(state.entered, expected);
    }

    fn assert_entries_stay(&self, expected: usize) {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, Duration::from_millis(100), |state| {
                state.entered == expected
            })
            .unwrap();
        assert_eq!(state.entered, expected, "concurrent writes interleaved");
    }

    fn release(&self) {
        self.state.lock().unwrap().blocked = false;
        self.changed.notify_all();
    }

    fn writes(&self) -> Vec<(u32, Vec<u8>)> {
        self.state.lock().unwrap().writes.clone()
    }

    fn max_active(&self) -> usize {
        self.state.lock().unwrap().max_active
    }
}

impl PtySink for GatePty {
    fn write(&self, pty_id: u32, bytes: &[u8]) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.entered += 1;
        state.active += 1;
        state.max_active = state.max_active.max(state.active);
        self.changed.notify_all();
        while state.blocked {
            state = self.changed.wait(state).unwrap();
        }
        state.writes.push((pty_id, bytes.to_vec()));
        state.active -= 1;
        self.changed.notify_all();
        Ok(())
    }
}

#[derive(Default)]
struct NoopPersistence;

impl NamePersistence for NoopPersistence {
    fn persist(&self, _request: PersistNameRequest) -> Result<(), String> {
        Ok(())
    }
}

struct ChannelPersistence {
    sender: Mutex<mpsc::Sender<PersistNameRequest>>,
}

impl NamePersistence for ChannelPersistence {
    fn persist(&self, request: PersistNameRequest) -> Result<(), String> {
        self.sender
            .lock()
            .unwrap()
            .send(request)
            .map_err(|error| error.to_string())
    }
}

struct FrozenClock(Instant);

impl Default for FrozenClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}

impl Clock for FrozenClock {
    fn now(&self) -> Instant {
        self.0
    }
}

fn catalog(id: &str, name: &str, private: bool) -> CatalogRecord {
    CatalogRecord {
        terminal_id: id.into(),
        address_name: Some(name.into()),
        private,
    }
}

fn service(
    pty: Arc<GatePty>,
    persistence: Arc<dyn NamePersistence>,
    records: Vec<CatalogRecord>,
) -> Arc<ControlService> {
    let ids = records
        .iter()
        .map(|record| record.terminal_id.clone())
        .collect::<Vec<_>>();
    let service = Arc::new(ControlService::new(
        pty,
        persistence,
        Arc::new(FrozenClock::default()),
    ));
    service.sync_catalog(records).unwrap();
    for (index, id) in ids.iter().enumerate() {
        service.mark_live(id, (index + 1) as u32).unwrap();
    }
    service
}

fn endpoint(label: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        r"\\.\pipe\terax-control-matrix-{}-{label}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

fn start_server(service: Arc<ControlService>, label: &str) -> (String, PipeServer) {
    let endpoint = endpoint(label);
    let handler = Arc::clone(&service);
    let server =
        PipeServer::spawn(endpoint.clone(), move |frame| handler.handle_frame(&frame)).unwrap();
    (endpoint, server)
}

fn encoded(endpoint: &str, token: &str, request_id: &str, command: CliCommand) -> Vec<u8> {
    let (_, request) = build_request(&command, Some(endpoint), Some(token), request_id).unwrap();
    serde_json::to_vec(&request).unwrap()
}

fn request(endpoint: &str, token: &str, request_id: &str, command: CliCommand) -> ControlResponse {
    let frame = encoded(endpoint, token, request_id, command);
    let response = call(endpoint, &frame, Duration::from_secs(2)).unwrap();
    serde_json::from_slice(&response).unwrap()
}

fn send(target: &str, message: &str) -> CliCommand {
    CliCommand::Send {
        target: target.into(),
        message: message.into(),
        json: false,
    }
}

fn error_code(response: ControlResponse) -> ErrorCode {
    assert!(!response.ok, "expected failure, got {response:?}");
    response.error.unwrap().code
}

#[test]
fn authenticated_send_returns_after_exact_writer_completion() {
    let pty = Arc::new(GatePty::blocked());
    let service = service(
        pty.clone(),
        Arc::new(NoopPersistence),
        vec![
            catalog("source", "agent-a", false),
            catalog("target", "agent-b", false),
        ],
    );
    let token = service.issue_credential("source").unwrap();
    let (endpoint, server) = start_server(service, "completion");
    let frame = encoded(
        &endpoint,
        &token,
        "send-completion",
        send("agent-b", "done"),
    );
    let (finished_tx, finished_rx) = mpsc::channel();
    let request_endpoint = endpoint.clone();
    let worker = thread::spawn(move || {
        let response = call(&request_endpoint, &frame, Duration::from_secs(2)).unwrap();
        finished_tx.send(response).unwrap();
    });

    pty.wait_for_entries(1);
    assert!(matches!(
        finished_rx.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    pty.release();
    let response: ControlResponse =
        serde_json::from_slice(&finished_rx.recv_timeout(Duration::from_secs(2)).unwrap()).unwrap();
    worker.join().unwrap();
    server.shutdown();

    assert!(response.ok);
    assert_eq!(
        pty.writes(),
        vec![(2, b"[terax from agent-a] done\r".to_vec())]
    );
}

#[test]
fn forged_expired_and_cross_instance_tokens_fail() {
    let records = vec![
        catalog("source", "agent-a", false),
        catalog("target", "agent-b", false),
    ];
    let service_a = service(
        Arc::new(GatePty::default()),
        Arc::new(NoopPersistence),
        records.clone(),
    );
    let service_b = service(
        Arc::new(GatePty::default()),
        Arc::new(NoopPersistence),
        records,
    );
    let expired = service_a.issue_credential("source").unwrap();
    let cross_instance = service_b.issue_credential("source").unwrap();
    let (endpoint, server) = start_server(service_a.clone(), "tokens");

    let forged = request(
        &endpoint,
        "forged-token",
        "forged",
        CliCommand::List { json: false },
    );
    let cross = request(
        &endpoint,
        &cross_instance,
        "cross",
        CliCommand::List { json: false },
    );
    service_a.begin_close_by_pty(1).unwrap();
    service_a.finish_close_by_pty(1).unwrap();
    let expired = request(
        &endpoint,
        &expired,
        "expired",
        CliCommand::List { json: false },
    );
    server.shutdown();

    assert_eq!(error_code(forged), ErrorCode::AuthFailed);
    assert_eq!(error_code(cross), ErrorCode::AuthFailed);
    assert_eq!(error_code(expired), ErrorCode::AuthFailed);
}

#[test]
fn concurrent_name_claims_have_one_winner() {
    let (persist_tx, persist_rx) = mpsc::channel();
    let persistence = Arc::new(ChannelPersistence {
        sender: Mutex::new(persist_tx),
    });
    let service = service(
        Arc::new(GatePty::default()),
        persistence,
        vec![
            catalog("source-a", "old-a", false),
            catalog("source-b", "old-b", false),
        ],
    );
    let token_a = service.issue_credential("source-a").unwrap();
    let token_b = service.issue_credential("source-b").unwrap();
    let (endpoint, server) = start_server(service.clone(), "names");
    let barrier = Arc::new(Barrier::new(3));

    let spawn_claim = |token: String, id: &'static str| {
        let endpoint = endpoint.clone();
        let barrier = barrier.clone();
        thread::spawn(move || {
            barrier.wait();
            request(
                &endpoint,
                &token,
                id,
                CliCommand::Name {
                    name: "shared-name".into(),
                    json: false,
                },
            )
        })
    };
    let first = spawn_claim(token_a, "claim-a");
    let second = spawn_claim(token_b, "claim-b");
    barrier.wait();

    let pending = persist_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    service.ack_name(&pending.request_id, None).unwrap();
    let responses = [first.join().unwrap(), second.join().unwrap()];
    server.shutdown();

    assert_eq!(responses.iter().filter(|response| response.ok).count(), 1);
    assert_eq!(
        responses
            .iter()
            .filter_map(|response| response.error.as_ref().map(|error| error.code))
            .collect::<Vec<_>>(),
        vec![ErrorCode::NameInUse]
    );
    assert!(persist_rx.try_recv().is_err());
}

#[test]
fn concurrent_target_writes_do_not_interleave() {
    let pty = Arc::new(GatePty::blocked());
    let service = service(
        pty.clone(),
        Arc::new(NoopPersistence),
        vec![
            catalog("source", "agent-a", false),
            catalog("target", "agent-b", false),
        ],
    );
    let token = service.issue_credential("source").unwrap();
    let (endpoint, server) = start_server(service, "serialization");

    let spawn_send = |id: &'static str, message: &'static str| {
        let endpoint = endpoint.clone();
        let token = token.clone();
        thread::spawn(move || request(&endpoint, &token, id, send("agent-b", message)))
    };
    let first = spawn_send("write-a", "first");
    pty.wait_for_entries(1);
    let second = spawn_send("write-b", "second");
    pty.assert_entries_stay(1);
    pty.release();

    assert!(first.join().unwrap().ok);
    assert!(second.join().unwrap().ok);
    server.shutdown();
    assert_eq!(pty.max_active(), 1);
    let writes = pty.writes();
    assert_eq!(writes.len(), 2);
    assert!(writes.iter().all(|(_, bytes)| bytes.ends_with(b"\r")));
}

#[test]
fn send_close_race_has_only_documented_outcomes() {
    let pty = Arc::new(GatePty::blocked());
    let service = service(
        pty.clone(),
        Arc::new(NoopPersistence),
        vec![
            catalog("source", "agent-a", false),
            catalog("target", "agent-b", false),
        ],
    );
    let token = service.issue_credential("source").unwrap();
    let (endpoint, server) = start_server(service.clone(), "close-race");
    let request_endpoint = endpoint.clone();
    let request_token = token.clone();
    let in_flight = thread::spawn(move || {
        request(
            &request_endpoint,
            &request_token,
            "before-close",
            send("agent-b", "accepted"),
        )
    });

    pty.wait_for_entries(1);
    service.begin_close_by_pty(2).unwrap();
    let rejected = request(
        &endpoint,
        &token,
        "after-close",
        send("agent-b", "rejected"),
    );
    assert_eq!(error_code(rejected), ErrorCode::TargetNotLive);
    pty.release();
    assert!(in_flight.join().unwrap().ok);
    service.finish_close_by_pty(2).unwrap();
    server.shutdown();
    assert_eq!(pty.writes().len(), 1);
}

#[test]
fn private_source_can_send_but_private_target_is_masked() {
    let pty = Arc::new(GatePty::default());
    let service = service(
        pty.clone(),
        Arc::new(NoopPersistence),
        vec![
            catalog("private-source", "agent-a", true),
            catalog("public-target", "agent-b", false),
            catalog("private-target", "agent-c", true),
        ],
    );
    let token = service.issue_credential("private-source").unwrap();
    let (endpoint, server) = start_server(service, "private");

    assert!(
        request(
            &endpoint,
            &token,
            "private-source-send",
            send("agent-b", "allowed")
        )
        .ok
    );
    assert_eq!(
        error_code(request(
            &endpoint,
            &token,
            "private-target-send",
            send("agent-c", "masked")
        )),
        ErrorCode::TargetNotFound
    );
    server.shutdown();
    assert_eq!(pty.writes().len(), 1);
}

#[test]
fn capacity_and_rate_limits_reject_without_offline_queue() {
    let blocked_pty = Arc::new(GatePty::blocked());
    let blocked_service = service(
        blocked_pty.clone(),
        Arc::new(NoopPersistence),
        vec![
            catalog("source", "agent-a", false),
            catalog("target", "agent-b", false),
        ],
    );
    let blocked_token = blocked_service.issue_credential("source").unwrap();
    let blocked_endpoint = endpoint("capacity");
    let handler = blocked_service.clone();
    let capacity_server =
        PipeServer::spawn_with_connection_limit(blocked_endpoint.clone(), 1, move |frame| {
            handler.handle_frame(&frame)
        })
        .unwrap();
    let first_frame = encoded(
        &blocked_endpoint,
        &blocked_token,
        "capacity-first",
        send("agent-b", "first"),
    );
    let first_endpoint = blocked_endpoint.clone();
    let first =
        thread::spawn(move || call(&first_endpoint, &first_frame, Duration::from_secs(2)).unwrap());
    blocked_pty.wait_for_entries(1);
    let busy_frame = encoded(
        &blocked_endpoint,
        &blocked_token,
        "capacity-second",
        send("agent-b", "second"),
    );
    let busy = call(&blocked_endpoint, &busy_frame, Duration::from_millis(100)).unwrap_err();
    assert_eq!(busy.code(), ErrorCode::ServerBusy);
    blocked_pty.release();
    first.join().unwrap();
    capacity_server.shutdown();
    assert_eq!(blocked_pty.writes().len(), 1);

    let rate_pty = Arc::new(GatePty::default());
    let rate_service = service(
        rate_pty.clone(),
        Arc::new(NoopPersistence),
        vec![
            catalog("source", "agent-a", false),
            catalog("target", "agent-b", false),
        ],
    );
    let rate_token = rate_service.issue_credential("source").unwrap();
    let (rate_endpoint, rate_server) = start_server(rate_service, "rate");
    let responses = (0..41)
        .map(|index| {
            request(
                &rate_endpoint,
                &rate_token,
                &format!("rate-{index}"),
                send("agent-b", "message"),
            )
        })
        .collect::<Vec<_>>();
    rate_server.shutdown();

    assert_eq!(responses.iter().filter(|response| response.ok).count(), 40);
    assert_eq!(
        responses
            .iter()
            .filter_map(|response| response.error.as_ref().map(|error| error.code))
            .collect::<Vec<_>>(),
        vec![ErrorCode::RateLimited]
    );
    assert_eq!(rate_pty.writes().len(), 40);
}
