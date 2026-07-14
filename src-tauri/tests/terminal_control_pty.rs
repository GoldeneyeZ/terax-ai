#![cfg(windows)]

use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Instant;
use terax_lib::modules::terminal_control::{
    CatalogRecord, Clock, ControlRequest, ControlService, ErrorCode, NamePersistence, PtySink,
    RecordState, ResponseData, SendPayload, PROTOCOL_VERSION,
};
use terax_lib::modules::workspace::WorkspaceEnv;

#[derive(Default)]
struct BlockingPty {
    state: Mutex<WriteState>,
    changed: Condvar,
}

#[derive(Default)]
struct WriteState {
    started: bool,
    released: bool,
    writes: Vec<(u32, Vec<u8>)>,
}

impl BlockingPty {
    fn wait_until_started(&self) {
        let state = self.state.lock().unwrap();
        let (state, wait) = self
            .changed
            .wait_timeout_while(state, std::time::Duration::from_secs(1), |state| {
                !state.started
            })
            .unwrap();
        assert!(!wait.timed_out(), "PTY write did not start");
        drop(state);
    }

    fn release(&self) {
        self.state.lock().unwrap().released = true;
        self.changed.notify_all();
    }
}

impl PtySink for BlockingPty {
    fn write(&self, pty_id: u32, bytes: &[u8]) -> Result<(), String> {
        let mut state = self.state.lock().unwrap();
        state.started = true;
        self.changed.notify_all();
        while !state.released {
            state = self.changed.wait(state).unwrap();
        }
        state.writes.push((pty_id, bytes.to_vec()));
        Ok(())
    }
}

#[derive(Default)]
struct NoopPersistence;

impl NamePersistence for NoopPersistence {
    fn persist(
        &self,
        _request: terax_lib::modules::terminal_control::PersistNameRequest,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct TestClock;

impl Clock for TestClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

fn request(token: &str) -> ControlRequest {
    ControlRequest::Send {
        version: PROTOCOL_VERSION,
        request_id: "send-1".into(),
        source_token: token.into(),
        payload: SendPayload {
            target: "agent-b".into(),
            message: "review commit".into(),
        },
    }
}

fn service(pty: Arc<BlockingPty>) -> Arc<ControlService> {
    let service = Arc::new(ControlService::new(
        pty,
        Arc::new(NoopPersistence),
        Arc::new(TestClock),
    ));
    service
        .sync_catalog(vec![
            CatalogRecord {
                terminal_id: "pane-a".into(),
                address_name: Some("agent-a".into()),
                private: false,
            },
            CatalogRecord {
                terminal_id: "pane-b".into(),
                address_name: Some("agent-b".into()),
                private: false,
            },
        ])
        .unwrap();
    service
}

#[test]
fn native_spawn_env_contains_pane_capability_and_cli_path() {
    let pty = Arc::new(BlockingPty::default());
    let service = service(pty);
    let credential = service
        .prepare_spawn("pane-a", Some("agent-a"), false, &WorkspaceEnv::Local)
        .unwrap()
        .unwrap();
    let vars = credential.variables();
    let cli_dir = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    assert_eq!(vars["TERAX_PANE_ID"], "pane-a");
    assert_eq!(vars["TERAX_IPC_ENDPOINT"], service.endpoint());
    assert_eq!(vars["TERAX_IPC_TOKEN"].len(), 43);
    assert!(vars["PATH"].starts_with(&format!("{cli_dir};")));
}

#[test]
fn wsl_spawn_gets_no_control_environment_and_never_becomes_live() {
    let pty = Arc::new(BlockingPty::default());
    let service = service(pty);

    let credential = service
        .prepare_spawn(
            "pane-a",
            Some("agent-a"),
            false,
            &WorkspaceEnv::Wsl {
                distro: "Ubuntu".into(),
            },
        )
        .unwrap();

    assert!(credential.is_none());
    assert_eq!(service.record_state("pane-a"), Some(RecordState::Persisted));
}

#[test]
fn prepared_capability_authenticates_only_after_session_activation() {
    let pty = Arc::new(BlockingPty::default());
    let service = service(pty);
    let source = service
        .prepare_spawn("pane-a", Some("agent-a"), false, &WorkspaceEnv::Local)
        .unwrap()
        .unwrap();
    let token = source.variables()["TERAX_IPC_TOKEN"].clone();

    let before = service.handle_request(request(&token));
    assert_eq!(before.error.unwrap().code, ErrorCode::AuthFailed);

    service.activate_spawn(&source, 1).unwrap();
    let after = service.handle_request(request(&token));
    assert_ne!(after.error.unwrap().code, ErrorCode::AuthFailed);
}

#[test]
fn send_already_holding_a_writer_may_finish_while_close_rejects_later_sends() {
    let pty = Arc::new(BlockingPty::default());
    let service = service(pty.clone());
    let source = service
        .prepare_spawn("pane-a", Some("agent-a"), false, &WorkspaceEnv::Local)
        .unwrap()
        .unwrap();
    let target = service
        .prepare_spawn("pane-b", Some("agent-b"), false, &WorkspaceEnv::Local)
        .unwrap()
        .unwrap();
    service.activate_spawn(&source, 1).unwrap();
    service.activate_spawn(&target, 2).unwrap();

    let token = source.variables()["TERAX_IPC_TOKEN"].clone();
    let request_service = service.clone();
    let request_token = token.clone();
    let send = thread::spawn(move || request_service.handle_request(request(&request_token)));
    pty.wait_until_started();

    service.begin_close_by_pty(2).unwrap();
    let rejected = service.handle_request(request(&token));
    assert_eq!(rejected.error.unwrap().code, ErrorCode::TargetNotLive);

    service.finish_close_by_pty(2).unwrap();
    assert_eq!(service.record_state("pane-b"), Some(RecordState::Exited));
    pty.release();
    let completed = send.join().unwrap();
    assert_eq!(
        completed.data,
        Some(ResponseData::Send {
            target: "agent-b".into()
        })
    );
}

#[test]
fn closing_a_source_revokes_its_capability() {
    let pty = Arc::new(BlockingPty::default());
    let service = service(pty);
    let source = service
        .prepare_spawn("pane-a", Some("agent-a"), false, &WorkspaceEnv::Local)
        .unwrap()
        .unwrap();
    service.activate_spawn(&source, 1).unwrap();
    let token = source.variables()["TERAX_IPC_TOKEN"].clone();

    service.begin_close_by_pty(1).unwrap();

    let response = service.handle_request(request(&token));
    assert_eq!(response.error.unwrap().code, ErrorCode::AuthFailed);
}
