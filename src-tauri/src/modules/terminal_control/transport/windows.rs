use std::ffi::c_void;
use std::fmt;
use std::fs::File;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    LocalFree, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, ERROR_SEM_TIMEOUT, GENERIC_READ,
    GENERIC_WRITE, HANDLE, HLOCAL, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::super::framing::{read_frame, write_frame};
use super::super::{ErrorCode, MAX_FRAME_BYTES};

pub const MAX_CONNECTIONS: usize = 32;
pub const SERVER_PIPE_MODE: u32 =
    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;

struct ServerState {
    shutdown: AtomicBool,
    active: AtomicUsize,
}

pub struct PipeServer {
    endpoint: String,
    state: Arc<ServerState>,
    accept_thread: Option<JoinHandle<()>>,
}

impl PipeServer {
    pub fn spawn<F>(endpoint: String, handler: F) -> io::Result<Self>
    where
        F: Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
    {
        Self::spawn_with_connection_limit(endpoint, MAX_CONNECTIONS, handler)
    }

    pub fn spawn_with_connection_limit<F>(
        endpoint: String,
        connection_limit: usize,
        handler: F,
    ) -> io::Result<Self>
    where
        F: Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
    {
        if !(1..=MAX_CONNECTIONS).contains(&connection_limit) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "connection limit must be between 1 and 32",
            ));
        }

        let pipe_name = wide_null(&endpoint)?;
        let state = Arc::new(ServerState {
            shutdown: AtomicBool::new(false),
            active: AtomicUsize::new(0),
        });
        let accept_state = Arc::clone(&state);
        let handler = Arc::new(handler);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let accept_thread = thread::Builder::new()
            .name("terminal-control-pipe".to_owned())
            .spawn(move || {
                accept_connections(
                    &pipe_name,
                    connection_limit,
                    accept_state,
                    handler,
                    ready_sender,
                );
            })?;

        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                endpoint,
                state,
                accept_thread: Some(accept_thread),
            }),
            Ok(Err(error)) => {
                let _ = accept_thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = accept_thread.join();
                Err(io::Error::other(
                    "pipe accept thread stopped during startup",
                ))
            }
        }
    }

    pub fn active_connections(&self) -> usize {
        self.state.active.load(Ordering::Acquire)
    }

    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let Some(accept_thread) = self.accept_thread.take() else {
            return;
        };

        self.state.shutdown.store(true, Ordering::Release);
        let waker = open_client(&self.endpoint, Duration::from_millis(100)).ok();
        let _ = accept_thread.join();
        drop(waker);
    }
}

impl Drop for PipeServer {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
pub struct TransportError {
    operation: &'static str,
    code: ErrorCode,
    source: io::Error,
}

impl TransportError {
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn io_error(&self) -> &io::Error {
        &self.source
    }

    fn new(operation: &'static str, code: ErrorCode, source: io::Error) -> Self {
        Self {
            operation,
            code,
            source,
        }
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} during {}: {}",
            self.code, self.operation, self.source
        )
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

pub fn call(endpoint: &str, request: &[u8], timeout: Duration) -> Result<Vec<u8>, TransportError> {
    if request.len() > MAX_FRAME_BYTES {
        return Err(TransportError::new(
            "request validation",
            ErrorCode::InvalidRequest,
            io::Error::new(io::ErrorKind::InvalidInput, "frame too large"),
        ));
    }

    let mut pipe = open_client(endpoint, timeout).map_err(map_open_error)?;
    write_frame(&mut pipe, request).map_err(|error| map_connection_error("write", error))?;
    read_frame(&mut pipe).map_err(|error| map_connection_error("read", error))
}

pub fn current_user_pipe_sddl() -> io::Result<String> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token as RawHandle) };

    let mut required = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle() as HANDLE,
            TokenUser,
            null_mut(),
            0,
            &mut required,
        );
    }
    if required == 0 {
        return Err(io::Error::last_os_error());
    }

    let word_count = (required as usize).div_ceil(size_of::<usize>());
    let mut token_information = vec![0_usize; word_count];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle() as HANDLE,
            TokenUser,
            token_information.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }

    let token_user = unsafe { &*token_information.as_ptr().cast::<TOKEN_USER>() };
    let mut sid = null_mut();
    if unsafe { ConvertSidToStringSidW(token_user.User.Sid, &mut sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let sid = LocalAllocation::new(sid.cast())?;
    let sid = unsafe { wide_string(sid.as_ptr().cast()) }?;

    Ok(format!("D:P(A;;GA;;;{sid})"))
}

fn accept_connections<F>(
    pipe_name: &[u16],
    connection_limit: usize,
    state: Arc<ServerState>,
    handler: Arc<F>,
    ready: mpsc::SyncSender<io::Result<()>>,
) where
    F: Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
{
    let security = match current_user_security_descriptor() {
        Ok(security) => security,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    let mut ready = Some(ready);

    while !state.shutdown.load(Ordering::Acquire) {
        if state.active.load(Ordering::Acquire) >= connection_limit {
            thread::sleep(Duration::from_millis(2));
            continue;
        }

        let pipe = match create_pipe(pipe_name, connection_limit, &security) {
            Ok(pipe) => pipe,
            Err(error) => {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Err(error));
                    return;
                }
                continue;
            }
        };
        if let Some(ready) = ready.take() {
            let _ = ready.send(Ok(()));
        }

        let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle() as HANDLE, null_mut()) };
        if connected == 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32) {
                continue;
            }
        }
        if state.shutdown.load(Ordering::Acquire) {
            unsafe {
                DisconnectNamedPipe(pipe.as_raw_handle() as HANDLE);
            }
            break;
        }

        state.active.fetch_add(1, Ordering::AcqRel);
        let connection_state = Arc::clone(&state);
        let connection_handler = Arc::clone(&handler);
        let spawn_result = thread::Builder::new()
            .name("terminal-control-connection".to_owned())
            .spawn(move || {
                let _connection = ActiveConnection(connection_state);
                handle_connection(pipe, connection_handler);
            });
        if spawn_result.is_err() {
            state.active.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

fn handle_connection<F>(mut pipe: File, handler: Arc<F>)
where
    F: Fn(Vec<u8>) -> Vec<u8>,
{
    if let Ok(request) = read_frame(&mut pipe) {
        let response = handler(request);
        if write_frame(&mut pipe, &response).is_ok() {
            unsafe {
                FlushFileBuffers(pipe.as_raw_handle() as HANDLE);
            }
        }
    }
    unsafe {
        DisconnectNamedPipe(pipe.as_raw_handle() as HANDLE);
    }
}

struct ActiveConnection(Arc<ServerState>);

impl Drop for ActiveConnection {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}

fn create_pipe(
    pipe_name: &[u16],
    connection_limit: usize,
    security: &SecurityDescriptor,
) -> io::Result<File> {
    let attributes = security.attributes();
    let handle = unsafe {
        CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            SERVER_PIPE_MODE,
            connection_limit as u32,
            MAX_FRAME_BYTES as u32,
            MAX_FRAME_BYTES as u32,
            0,
            &attributes,
        )
    };
    file_from_handle(handle)
}

fn open_client(endpoint: &str, timeout: Duration) -> io::Result<File> {
    let pipe_name = wide_null(endpoint)?;
    if unsafe { WaitNamedPipeW(pipe_name.as_ptr(), timeout_millis(timeout)) } == 0 {
        return Err(io::Error::last_os_error());
    }

    let handle = unsafe {
        CreateFileW(
            pipe_name.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    file_from_handle(handle)
}

fn file_from_handle(handle: HANDLE) -> io::Result<File> {
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let handle = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
    Ok(File::from(handle))
}

fn map_open_error(error: io::Error) -> TransportError {
    let code = match error.raw_os_error() {
        Some(raw) if raw == ERROR_PIPE_BUSY as i32 || raw == ERROR_SEM_TIMEOUT as i32 => {
            ErrorCode::ServerBusy
        }
        _ => ErrorCode::TeraxUnavailable,
    };
    TransportError::new("connect", code, error)
}

fn map_connection_error(operation: &'static str, error: io::Error) -> TransportError {
    TransportError::new(operation, ErrorCode::TeraxUnavailable, error)
}

fn timeout_millis(timeout: Duration) -> u32 {
    timeout.as_millis().min(u32::MAX as u128) as u32
}

fn wide_null(value: &str) -> io::Result<Vec<u16>> {
    if value.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "pipe name contains a null character",
        ));
    }
    Ok(value.encode_utf16().chain([0]).collect())
}

fn current_user_security_descriptor() -> io::Result<SecurityDescriptor> {
    let sddl = wide_null(&current_user_pipe_sddl()?)?;
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }

    Ok(SecurityDescriptor(LocalAllocation::new(descriptor.cast())?))
}

struct SecurityDescriptor(LocalAllocation);

impl SecurityDescriptor {
    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.as_ptr(),
            bInheritHandle: 0,
        }
    }
}

struct LocalAllocation(*mut c_void);

impl LocalAllocation {
    fn new(pointer: *mut c_void) -> io::Result<Self> {
        if pointer.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(pointer))
        }
    }

    fn as_ptr(&self) -> *mut c_void {
        self.0
    }
}

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0 as HLOCAL);
        }
    }
}

unsafe fn wide_string(pointer: *const u16) -> io::Result<String> {
    if pointer.is_null() {
        return Err(io::Error::last_os_error());
    }

    let mut length = 0;
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    String::from_utf16(unsafe { std::slice::from_raw_parts(pointer, length) })
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
