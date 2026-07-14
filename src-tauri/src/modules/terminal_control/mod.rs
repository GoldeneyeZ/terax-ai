pub mod credentials;
pub mod directory;
pub mod framing;
pub mod protocol;
pub mod rate_limit;
pub mod service;
pub mod transport;

pub use credentials::Credentials;
pub use directory::{NameReservation, RecordState, TerminalDirectory, TerminalRecord};
pub use protocol::{
    build_envelope, validate_message, validate_name, ControlError, ControlRequest, ControlResponse,
    ErrorCode, ListPayload, NamePayload, ResponseData, SendPayload, MAX_FRAME_BYTES,
    MAX_MESSAGE_BYTES, PROTOCOL_VERSION,
};
pub use rate_limit::TokenBucket;
pub use service::{
    new_endpoint, CatalogRecord, Clock, ControlService, NamePersistence, PersistNameRequest,
    PtySink, SpawnCredential, SystemClock, TerminalControlState, PERSIST_NAME_EVENT,
};

#[tauri::command]
pub fn terminal_control_sync_catalog(
    state: tauri::State<'_, TerminalControlState>,
    records: Vec<CatalogRecord>,
) -> Result<(), String> {
    state
        .sync_catalog(records)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn terminal_control_ack_name(
    state: tauri::State<'_, TerminalControlState>,
    request_id: String,
    error: Option<String>,
) -> Result<(), String> {
    state.ack_name(&request_id, error)
}
