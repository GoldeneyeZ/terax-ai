use super::protocol::{validate_name, ErrorCode};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordState {
    Persisted,
    Live,
    Closing,
    Exited,
    Conflicted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalRecord {
    pub terminal_id: String,
    pub address_name: Option<String>,
    pub private: bool,
    pub state: RecordState,
    pub pty_id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameReservation {
    pub request_id: String,
    pub terminal_id: String,
    pub old_name: Option<String>,
    pub new_name: String,
}

#[derive(Debug, Default)]
pub struct TerminalDirectory {
    records: HashMap<String, TerminalRecord>,
    names: HashMap<String, String>,
    reservations: HashMap<String, NameReservation>,
    removed_live_records: HashSet<String>,
}

impl TerminalDirectory {
    pub fn sync_catalog(&mut self, records: Vec<TerminalRecord>) -> Result<(), ErrorCode> {
        let mut incoming = Vec::with_capacity(records.len());
        let mut terminal_ids = HashSet::with_capacity(records.len());

        for record in records {
            if record.terminal_id.is_empty() || !terminal_ids.insert(record.terminal_id.clone()) {
                return Err(ErrorCode::InvalidRequest);
            }
            let address_name = record
                .address_name
                .as_deref()
                .map(validate_name)
                .transpose()?;
            incoming.push((record.terminal_id, address_name, record.private));
        }

        let mut name_counts = HashMap::<String, usize>::new();
        for (_, name, _) in &incoming {
            if let Some(name) = name {
                *name_counts.entry(name.clone()).or_default() += 1;
            }
        }

        let mut next_records = HashMap::with_capacity(incoming.len() + self.records.len());
        for (terminal_id, mut address_name, private) in incoming {
            let conflicted = address_name
                .as_ref()
                .is_some_and(|name| name_counts.get(name).copied().unwrap_or_default() > 1);
            let previous = self.records.get(&terminal_id);

            if let Some(reservation) = self
                .reservations
                .values()
                .find(|reservation| reservation.terminal_id == terminal_id)
            {
                if address_name.as_deref() == Some(reservation.new_name.as_str()) {
                    address_name = reservation.old_name.clone();
                }
            }

            let (state, pty_id) = if conflicted {
                (
                    RecordState::Conflicted,
                    previous.and_then(|record| record.pty_id),
                )
            } else {
                match previous {
                    Some(record)
                        if matches!(record.state, RecordState::Live | RecordState::Closing) =>
                    {
                        (record.state, record.pty_id)
                    }
                    Some(record) if record.state == RecordState::Exited => {
                        (RecordState::Exited, None)
                    }
                    _ => (RecordState::Persisted, None),
                }
            };

            next_records.insert(
                terminal_id.clone(),
                TerminalRecord {
                    terminal_id,
                    address_name,
                    private,
                    state,
                    pty_id,
                },
            );
        }

        let incoming_ids = next_records.keys().cloned().collect::<HashSet<_>>();
        let mut next_removed_live = self.removed_live_records.clone();
        for (terminal_id, record) in &self.records {
            if incoming_ids.contains(terminal_id) {
                next_removed_live.remove(terminal_id);
                continue;
            }
            if matches!(record.state, RecordState::Live | RecordState::Closing) {
                let mut closing = record.clone();
                closing.state = RecordState::Closing;
                next_records.insert(terminal_id.clone(), closing);
                next_removed_live.insert(terminal_id.clone());
            }
        }

        let mut next_names = HashMap::new();
        for record in next_records.values() {
            if record.state == RecordState::Conflicted {
                continue;
            }
            if let Some(name) = &record.address_name {
                next_names.insert(name.clone(), record.terminal_id.clone());
            }
        }

        self.reservations.retain(|_, reservation| {
            next_records.contains_key(&reservation.terminal_id)
                && next_names
                    .get(&reservation.new_name)
                    .is_none_or(|owner| owner == &reservation.terminal_id)
        });
        self.records = next_records;
        self.names = next_names;
        self.removed_live_records = next_removed_live;
        Ok(())
    }

    pub fn reserve_name(
        &mut self,
        terminal_id: &str,
        requested_name: &str,
        request_id: &str,
    ) -> Result<NameReservation, ErrorCode> {
        let new_name = validate_name(requested_name)?;
        let record = self
            .records
            .get(terminal_id)
            .ok_or(ErrorCode::InvalidRequest)?;

        if let Some(existing) = self.reservations.get(request_id) {
            return if existing.terminal_id == terminal_id && existing.new_name == new_name {
                Ok(existing.clone())
            } else {
                Err(ErrorCode::InvalidRequest)
            };
        }
        if self
            .reservations
            .values()
            .any(|reservation| reservation.terminal_id == terminal_id)
        {
            return Err(ErrorCode::ServerBusy);
        }
        if self
            .names
            .get(&new_name)
            .is_some_and(|owner| owner != terminal_id)
            || self.reservations.values().any(|reservation| {
                reservation.new_name == new_name && reservation.terminal_id != terminal_id
            })
        {
            return Err(ErrorCode::NameInUse);
        }

        let reservation = NameReservation {
            request_id: request_id.to_owned(),
            terminal_id: terminal_id.to_owned(),
            old_name: record.address_name.clone(),
            new_name,
        };
        self.reservations
            .insert(request_id.to_owned(), reservation.clone());
        Ok(reservation)
    }

    pub fn commit_name(&mut self, request_id: &str) -> Result<NameReservation, ErrorCode> {
        let reservation = self
            .reservations
            .remove(request_id)
            .ok_or(ErrorCode::InvalidRequest)?;
        if self
            .names
            .get(&reservation.new_name)
            .is_some_and(|owner| owner != &reservation.terminal_id)
        {
            self.reservations.insert(request_id.to_owned(), reservation);
            return Err(ErrorCode::NameInUse);
        }

        let record = self
            .records
            .get_mut(&reservation.terminal_id)
            .ok_or(ErrorCode::InvalidRequest)?;
        if let Some(current_name) = record.address_name.take() {
            if self.names.get(&current_name) == Some(&reservation.terminal_id) {
                self.names.remove(&current_name);
            }
        }
        record.address_name = Some(reservation.new_name.clone());
        self.names.insert(
            reservation.new_name.clone(),
            reservation.terminal_id.clone(),
        );
        Ok(reservation)
    }

    pub fn rollback_name(&mut self, request_id: &str) -> Result<NameReservation, ErrorCode> {
        self.reservations
            .remove(request_id)
            .ok_or(ErrorCode::InvalidRequest)
    }

    pub fn owner(&self, name: &str) -> Option<&str> {
        let name = validate_name(name).ok()?;
        self.names.get(&name).map(String::as_str)
    }

    pub fn record(&self, terminal_id: &str) -> Option<&TerminalRecord> {
        self.records.get(terminal_id)
    }

    pub fn source_name(&self, terminal_id: &str) -> Result<String, ErrorCode> {
        self.records
            .get(terminal_id)
            .filter(|record| record.state != RecordState::Conflicted)
            .and_then(|record| record.address_name.clone())
            .ok_or(ErrorCode::SourceUnnamed)
    }

    pub fn resolve_target(&self, name: &str) -> Result<TerminalRecord, ErrorCode> {
        let name = validate_name(name).map_err(|_| ErrorCode::TargetNotFound)?;
        let terminal_id = self.names.get(&name).ok_or(ErrorCode::TargetNotFound)?;
        let record = self
            .records
            .get(terminal_id)
            .ok_or(ErrorCode::TargetNotFound)?;

        if record.private || record.state == RecordState::Conflicted {
            return Err(ErrorCode::TargetNotFound);
        }
        if record.state != RecordState::Live || record.pty_id.is_none() {
            return Err(ErrorCode::TargetNotLive);
        }
        Ok(record.clone())
    }

    pub fn list_targets(&self) -> Vec<String> {
        let mut names = self
            .records
            .values()
            .filter(|record| {
                !record.private
                    && record.state == RecordState::Live
                    && record.pty_id.is_some()
                    && record.address_name.is_some()
            })
            .filter_map(|record| record.address_name.clone())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names
    }

    pub fn mark_live(&mut self, terminal_id: &str, pty_id: u32) -> Result<(), ErrorCode> {
        let record = self
            .records
            .get_mut(terminal_id)
            .ok_or(ErrorCode::InvalidRequest)?;
        record.pty_id = Some(pty_id);
        if record.state != RecordState::Conflicted {
            record.state = RecordState::Live;
        }
        Ok(())
    }

    pub fn mark_closing(&mut self, terminal_id: &str) -> Result<(), ErrorCode> {
        let record = self
            .records
            .get_mut(terminal_id)
            .ok_or(ErrorCode::InvalidRequest)?;
        record.state = RecordState::Closing;
        Ok(())
    }

    pub fn mark_exited(&mut self, terminal_id: &str) -> Result<(), ErrorCode> {
        if self.removed_live_records.remove(terminal_id) {
            self.remove_record(terminal_id);
            return Ok(());
        }

        let record = self
            .records
            .get_mut(terminal_id)
            .ok_or(ErrorCode::InvalidRequest)?;
        record.state = RecordState::Exited;
        record.pty_id = None;
        Ok(())
    }

    fn remove_record(&mut self, terminal_id: &str) {
        if let Some(record) = self.records.remove(terminal_id) {
            if let Some(name) = record.address_name {
                if self
                    .names
                    .get(&name)
                    .is_some_and(|owner| owner == terminal_id)
                {
                    self.names.remove(&name);
                }
            }
        }
        self.reservations
            .retain(|_, reservation| reservation.terminal_id != terminal_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_record(id: &str, name: Option<&str>, private: bool) -> TerminalRecord {
        TerminalRecord {
            terminal_id: id.into(),
            address_name: name.map(str::to_owned),
            private,
            state: RecordState::Persisted,
            pty_id: None,
        }
    }

    fn hydrated_directory() -> TerminalDirectory {
        let mut directory = TerminalDirectory::default();
        directory
            .sync_catalog(vec![
                catalog_record("pane-a", None, false),
                catalog_record("pane-b", None, false),
            ])
            .unwrap();
        directory
    }

    #[test]
    fn duplicate_claim_keeps_existing_owner() {
        let mut directory = hydrated_directory();
        directory
            .reserve_name("pane-a", "agent-a", "req-1")
            .unwrap();
        directory.commit_name("req-1").unwrap();
        assert_eq!(
            directory.reserve_name("pane-b", "agent-a", "req-2"),
            Err(ErrorCode::NameInUse),
        );
        assert_eq!(directory.owner("agent-a"), Some("pane-a"));
    }

    #[test]
    fn reservation_is_canonical_and_rollback_keeps_the_committed_name() {
        let mut directory = hydrated_directory();
        directory
            .reserve_name("pane-a", "Agent-A", "req-1")
            .unwrap();
        directory.commit_name("req-1").unwrap();
        let reservation = directory
            .reserve_name("pane-a", "Agent-B", "req-2")
            .unwrap();

        assert_eq!(reservation.old_name.as_deref(), Some("agent-a"));
        assert_eq!(reservation.new_name, "agent-b");
        assert_eq!(directory.owner("agent-a"), Some("pane-a"));
        assert_eq!(directory.owner("agent-b"), None);

        directory.rollback_name("req-2").unwrap();
        assert_eq!(directory.owner("agent-a"), Some("pane-a"));
        assert_eq!(directory.owner("agent-b"), None);
    }

    #[test]
    fn private_and_conflicted_targets_are_masked() {
        let mut directory = TerminalDirectory::default();
        directory
            .sync_catalog(vec![
                catalog_record("private", Some("private-a"), true),
                catalog_record("conflict-1", Some("conflict-a"), false),
                catalog_record("conflict-2", Some("conflict-a"), false),
            ])
            .unwrap();
        directory.mark_live("private", 1).unwrap();

        assert_eq!(
            directory.resolve_target("private-a"),
            Err(ErrorCode::TargetNotFound)
        );
        assert_eq!(
            directory.resolve_target("conflict-a"),
            Err(ErrorCode::TargetNotFound)
        );
        assert_eq!(
            directory.record("conflict-1").unwrap().state,
            RecordState::Conflicted
        );
        assert_eq!(
            directory.record("conflict-2").unwrap().state,
            RecordState::Conflicted
        );
        assert_eq!(
            directory.source_name("conflict-1"),
            Err(ErrorCode::SourceUnnamed)
        );
        assert_eq!(
            directory.source_name("private"),
            Ok("private-a".to_string())
        );
    }

    #[test]
    fn inactive_public_name_is_reserved_but_not_live() {
        let mut directory = TerminalDirectory::default();
        directory
            .sync_catalog(vec![catalog_record("pane-a", Some("agent-a"), false)])
            .unwrap();

        assert_eq!(directory.owner("agent-a"), Some("pane-a"));
        assert_eq!(
            directory.resolve_target("agent-a"),
            Err(ErrorCode::TargetNotLive)
        );
        assert!(directory.list_targets().is_empty());
    }

    #[test]
    fn catalog_deletion_releases_an_inactive_name() {
        let mut directory = TerminalDirectory::default();
        directory
            .sync_catalog(vec![
                catalog_record("pane-a", Some("agent-a"), false),
                catalog_record("pane-b", None, false),
            ])
            .unwrap();

        directory
            .sync_catalog(vec![catalog_record("pane-b", None, false)])
            .unwrap();
        directory
            .reserve_name("pane-b", "agent-a", "req-1")
            .unwrap();
        directory.commit_name("req-1").unwrap();

        assert_eq!(directory.owner("agent-a"), Some("pane-b"));
        assert!(directory.record("pane-a").is_none());
    }

    #[test]
    fn live_record_omitted_by_sync_closes_before_removal() {
        let mut directory = TerminalDirectory::default();
        directory
            .sync_catalog(vec![catalog_record("pane-a", Some("agent-a"), false)])
            .unwrap();
        directory.mark_live("pane-a", 7).unwrap();

        directory.sync_catalog(Vec::new()).unwrap();
        let closing = directory.record("pane-a").unwrap();
        assert_eq!(closing.state, RecordState::Closing);
        assert_eq!(closing.pty_id, Some(7));
        assert_eq!(
            directory.resolve_target("agent-a"),
            Err(ErrorCode::TargetNotLive)
        );

        directory.mark_exited("pane-a").unwrap();
        assert!(directory.record("pane-a").is_none());
        assert_eq!(directory.owner("agent-a"), None);
    }

    #[test]
    fn sync_preserves_live_runtime_binding_and_lists_public_names_sorted() {
        let mut directory = TerminalDirectory::default();
        directory
            .sync_catalog(vec![
                catalog_record("pane-b", Some("bravo"), false),
                catalog_record("pane-a", Some("alpha"), false),
            ])
            .unwrap();
        directory.mark_live("pane-b", 2).unwrap();
        directory.mark_live("pane-a", 1).unwrap();

        directory
            .sync_catalog(vec![
                catalog_record("pane-a", Some("alpha"), false),
                catalog_record("pane-b", Some("bravo"), false),
            ])
            .unwrap();

        assert_eq!(directory.resolve_target("alpha").unwrap().pty_id, Some(1));
        assert_eq!(directory.list_targets(), vec!["alpha", "bravo"]);
    }

    #[test]
    fn invalid_catalog_does_not_replace_existing_state() {
        let mut directory = TerminalDirectory::default();
        directory
            .sync_catalog(vec![catalog_record("pane-a", Some("agent-a"), false)])
            .unwrap();

        assert_eq!(
            directory.sync_catalog(vec![catalog_record("pane-b", Some("not valid"), false)]),
            Err(ErrorCode::InvalidName)
        );
        assert_eq!(directory.owner("agent-a"), Some("pane-a"));
        assert!(directory.record("pane-b").is_none());
    }
}
