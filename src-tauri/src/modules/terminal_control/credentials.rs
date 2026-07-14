use super::protocol::ErrorCode;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use subtle::{Choice, ConditionallySelectable, ConstantTimeEq};

const TOKEN_BYTES: usize = 32;

#[derive(Debug)]
struct CredentialEntry {
    digest: [u8; 32],
    terminal_id: String,
}

#[derive(Debug, Default)]
pub struct Credentials {
    entries: Vec<CredentialEntry>,
}

impl Credentials {
    pub fn generate_token() -> Result<String, ErrorCode> {
        let mut random = [0_u8; TOKEN_BYTES];
        getrandom::fill(&mut random).map_err(|_| ErrorCode::Internal)?;
        Ok(URL_SAFE_NO_PAD.encode(random))
    }

    pub fn activate(&mut self, terminal_id: &str, token: &str) {
        self.entries.push(CredentialEntry {
            digest: token_digest(token),
            terminal_id: terminal_id.to_owned(),
        });
    }

    pub fn issue(&mut self, terminal_id: &str) -> Result<String, ErrorCode> {
        let token = Self::generate_token()?;
        self.activate(terminal_id, &token);
        Ok(token)
    }

    pub fn authenticate(&self, token: &str) -> Option<String> {
        let candidate = token_digest(token);
        let mut found = Choice::from(0);
        let mut selected = 0_u64;

        for (index, entry) in self.entries.iter().enumerate() {
            let matches = entry.digest.ct_eq(&candidate);
            let index = index as u64;
            selected = u64::conditional_select(&selected, &index, matches);
            found |= matches;
        }

        if bool::from(found) {
            self.entries
                .get(selected as usize)
                .map(|entry| entry.terminal_id.clone())
        } else {
            None
        }
    }

    pub fn revoke_pane(&mut self, terminal_id: &str) {
        self.entries
            .retain(|entry| entry.terminal_id != terminal_id);
    }

    pub fn revoke_all(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_authenticates_one_pane_then_revokes() {
        let mut credentials = Credentials::default();
        let token = credentials.issue("pane-a").unwrap();

        assert_eq!(credentials.authenticate(&token), Some("pane-a".to_string()));
        credentials.revoke_pane("pane-a");
        assert_eq!(credentials.authenticate(&token), None);
    }

    #[test]
    fn issued_tokens_are_url_safe_unique_and_not_stored_raw() {
        let mut credentials = Credentials::default();
        let first = credentials.issue("pane-a").unwrap();
        let second = credentials.issue("pane-b").unwrap();

        assert_eq!(first.len(), 43);
        assert!(first
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'));
        assert_ne!(first, second);
        assert!(credentials
            .entries
            .iter()
            .all(|entry| entry.digest.as_slice() != first.as_bytes()));
    }

    #[test]
    fn forged_token_is_checked_against_all_entries_without_authenticating() {
        let mut credentials = Credentials::default();
        credentials.issue("pane-a").unwrap();
        credentials.issue("pane-b").unwrap();
        credentials.issue("pane-c").unwrap();

        assert_eq!(credentials.authenticate("forged-token"), None);
        assert_eq!(credentials.len(), 3);
    }

    #[test]
    fn revoking_a_pane_removes_all_of_its_live_tokens_only() {
        let mut credentials = Credentials::default();
        let old = credentials.issue("pane-a").unwrap();
        let current = credentials.issue("pane-a").unwrap();
        let other = credentials.issue("pane-b").unwrap();

        credentials.revoke_pane("pane-a");

        assert_eq!(credentials.authenticate(&old), None);
        assert_eq!(credentials.authenticate(&current), None);
        assert_eq!(credentials.authenticate(&other), Some("pane-b".into()));
    }
}
