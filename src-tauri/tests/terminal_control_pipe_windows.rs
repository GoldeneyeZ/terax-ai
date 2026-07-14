#![cfg(windows)]

use std::fs::OpenOptions;
use std::io::{ErrorKind, Read, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use terax_lib::modules::terminal_control::framing::{read_frame, write_frame};
use terax_lib::modules::terminal_control::transport::windows::{
    call, current_user_pipe_sddl, PipeServer, MAX_CONNECTIONS, SERVER_PIPE_MODE,
};
use terax_lib::modules::terminal_control::{ErrorCode, MAX_FRAME_BYTES};
use windows_sys::Win32::System::Pipes::PIPE_REJECT_REMOTE_CLIENTS;

fn unique_test_endpoint() -> String {
    static NEXT_ENDPOINT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    format!(
        r"\\.\pipe\terax-control-test-{}-{}",
        std::process::id(),
        NEXT_ENDPOINT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

#[test]
fn frame_round_trip() {
    let payload = br#"{"version":1}"#;
    let mut bytes = Vec::new();
    write_frame(&mut bytes, payload).unwrap();
    assert_eq!(read_frame(&mut bytes.as_slice()).unwrap(), payload);
}

#[test]
fn oversized_frame_is_rejected_before_body_read() {
    let mut bytes = ((MAX_FRAME_BYTES as u32) + 1).to_le_bytes().to_vec();
    bytes.extend_from_slice(b"ignored");

    assert_eq!(
        read_frame(&mut bytes.as_slice()).unwrap_err().kind(),
        ErrorKind::InvalidData
    );
}

#[test]
fn oversized_frame_is_rejected_before_write() {
    let mut bytes = Vec::new();
    let payload = vec![0_u8; MAX_FRAME_BYTES + 1];

    assert_eq!(
        write_frame(&mut bytes, &payload).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert!(bytes.is_empty());
}

#[test]
fn current_user_security_is_protected_and_user_scoped() {
    let sddl = current_user_pipe_sddl().unwrap();

    assert!(sddl.starts_with("D:P(A;;GA;;;S-1-"));
    assert!(sddl.ends_with(')'));
    assert_eq!(sddl.matches("(A;;GA;;;").count(), 1);
}

#[test]
fn server_rejects_remote_clients_and_caps_connections() {
    assert_ne!(SERVER_PIPE_MODE & PIPE_REJECT_REMOTE_CLIENTS, 0);
    assert_eq!(MAX_CONNECTIONS, 32);
}

#[test]
fn current_process_can_round_trip_one_frame() {
    let endpoint = unique_test_endpoint();
    let server = PipeServer::spawn(endpoint.clone(), |request| request).unwrap();

    let response = call(
        &endpoint,
        br#"{"operation":"list"}"#,
        Duration::from_secs(2),
    )
    .unwrap();

    assert_eq!(response, br#"{"operation":"list"}"#);
    server.shutdown();
}

#[test]
fn server_rejects_oversized_frame_and_keeps_accepting() {
    let endpoint = unique_test_endpoint();
    let server = PipeServer::spawn(endpoint.clone(), |request| request).unwrap();
    let mut raw_client = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&endpoint)
        .unwrap();

    raw_client
        .write_all(&((MAX_FRAME_BYTES as u32) + 1).to_le_bytes())
        .unwrap();
    raw_client.flush().unwrap();
    let mut byte = [0_u8; 1];
    assert!(matches!(raw_client.read(&mut byte), Ok(0) | Err(_)));
    drop(raw_client);

    assert_eq!(
        call(&endpoint, b"still-alive", Duration::from_secs(2)).unwrap(),
        b"still-alive"
    );
    server.shutdown();
}

#[test]
fn client_reports_server_busy_after_timeout_at_connection_cap() {
    let endpoint = unique_test_endpoint();
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let handler_release = Arc::clone(&release);
    let server = PipeServer::spawn_with_connection_limit(endpoint.clone(), 1, move |request| {
        let (lock, wake) = &*handler_release;
        let mut released = lock.lock().unwrap();
        while !*released {
            released = wake.wait(released).unwrap();
        }
        request
    })
    .unwrap();
    let first_endpoint = endpoint.clone();
    let first =
        thread::spawn(move || call(&first_endpoint, b"first", Duration::from_secs(2)).unwrap());

    let deadline = Instant::now() + Duration::from_secs(2);
    while server.active_connections() != 1 && Instant::now() < deadline {
        thread::yield_now();
    }
    assert_eq!(server.active_connections(), 1);

    let error = call(&endpoint, b"second", Duration::from_millis(50)).unwrap_err();
    assert_eq!(error.code(), ErrorCode::ServerBusy);

    let (lock, wake) = &*release;
    *lock.lock().unwrap() = true;
    wake.notify_all();
    assert_eq!(first.join().unwrap(), b"first");
    server.shutdown();
}

#[test]
fn shutdown_wakes_accept_loop_and_removes_endpoint() {
    let endpoint = unique_test_endpoint();
    let server = PipeServer::spawn(endpoint.clone(), |request| request).unwrap();

    let started = Instant::now();
    server.shutdown();
    assert!(started.elapsed() < Duration::from_secs(1));

    let error = call(&endpoint, b"after-shutdown", Duration::from_millis(50)).unwrap_err();
    assert_eq!(error.code(), ErrorCode::TeraxUnavailable);
}
