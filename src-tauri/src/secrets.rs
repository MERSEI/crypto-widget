//! The one place API credentials are stored.
//!
//! Until now this app had nothing to hide: every endpoint was public and `settings.json` could
//! be posted in a bug report. Futures access and partner statistics change that, so secrets get
//! their own home — the OS credential store (Windows Credential Manager via `keyring`) — and
//! never touch `settings.json`, the TTL cache, or a log line.
//!
//! Two structural guards, both deliberate:
//!
//! - [`Credential`] does not implement `Serialize`. A `#[tauri::command]` can only return types
//!   that serialize, so a secret cannot be handed to the renderer even by accident — the code
//!   would not compile. What the UI gets instead is [`CredentialStatus`].
//! - Credentials are namespaced (`futures-mainnet`, `futures-testnet`, `referral-<partner>`),
//!   so a read for one venue can never return another's key.

use serde::Serialize;

/// Service name under which every credential is filed in the OS store. Matches the app's
/// bundle identifier, so the entries are recognisable in the Windows Credential Manager UI.
const SERVICE: &str = "com.flowe.crypto-widget";

/// An API key/secret pair. Intentionally **not** `Serialize` — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub key: String,
    pub secret: String,
}

/// What the renderer is allowed to know about a stored credential: that it exists, and enough
/// of the key to recognise which one it is.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub present: bool,
    /// `vmPU…f3Kq`, or `None` when nothing is stored.
    pub masked_key: Option<String>,
}

/// Storage envelope. Private, so the JSON layout of a secret never leaks into another module's
/// type signature.
#[derive(serde::Serialize, serde::Deserialize)]
struct Stored {
    key: String,
    secret: String,
}

fn entry(namespace: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, namespace).map_err(|e| format!("credential store unavailable: {e}"))
}

pub fn save(namespace: &str, credential: &Credential) -> Result<(), String> {
    if credential.key.trim().is_empty() || credential.secret.trim().is_empty() {
        return Err("both the API key and the secret are required".into());
    }
    let stored = Stored {
        key: credential.key.trim().to_string(),
        secret: credential.secret.trim().to_string(),
    };
    let json = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
    entry(namespace)?
        .set_password(&json)
        .map_err(|e| format!("could not store the credential: {e}"))
}

/// Reads a credential back. A missing entry is `None`, not an error — "no key configured yet"
/// is the normal state for a feature that ships switched off.
pub fn load(namespace: &str) -> Option<Credential> {
    let raw = entry(namespace).ok()?.get_password().ok()?;
    let stored: Stored = serde_json::from_str(&raw).ok()?;
    Some(Credential {
        key: stored.key,
        secret: stored.secret,
    })
}

/// Removes a credential. Deleting one that was never there is a success, not a failure — the
/// caller asked for it to be gone, and it is.
pub fn delete(namespace: &str) -> Result<(), String> {
    match entry(namespace)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("could not delete the credential: {e}")),
    }
}

pub fn status(namespace: &str) -> CredentialStatus {
    match load(namespace) {
        Some(credential) => CredentialStatus {
            present: true,
            masked_key: Some(mask(&credential.key)),
        },
        None => CredentialStatus::default(),
    }
}

/// Shows the head and tail of a key so a user with several can tell which one is loaded,
/// without putting a usable secret on screen — or in a screenshot.
///
/// Binance keys are 64 characters; anything short enough that head + tail would reveal most of
/// it is masked wholesale instead.
pub fn mask(key: &str) -> String {
    const EDGE: usize = 4;
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= EDGE * 3 {
        return "•".repeat(chars.len().min(8));
    }
    let head: String = chars[..EDGE].iter().collect();
    let tail: String = chars[chars.len() - EDGE..].iter().collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_a_binance_length_key_to_its_edges() {
        let key = "vmPUZE6mv9SD5VNHk4HlWFsOr6aKE2zvsw0MuIgwCIPy6utIco14y7Ju91duEh8A";
        assert_eq!(mask(key), "vmPU…Eh8A");
    }

    #[test]
    fn a_short_key_is_hidden_entirely() {
        // Nothing recognisable may survive: showing 4+4 of a 10-character key is showing the key.
        assert_eq!(mask("secret1234"), "••••••••");
        assert_eq!(mask(""), "");
    }

    #[test]
    fn masking_never_leaks_the_middle() {
        let key = "0123456789abcdefghij";
        let masked = mask(key);
        assert!(!masked.contains("456789abcdefgh"), "{masked}");
    }

    #[test]
    fn a_blank_credential_is_refused_before_it_reaches_the_store() {
        let blank = Credential {
            key: "   ".into(),
            secret: "abc".into(),
        };
        assert!(save("crypto-widget-test-blank", &blank).is_err());
    }

    /// Round-trips through the real Windows Credential Manager. Ignored by default: it writes
    /// to the developer's actual credential store, which is not something a CI run — or a
    /// `cargo test` someone fires off to check an unrelated change — should be doing.
    /// Run explicitly with `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn round_trips_through_the_os_credential_store() {
        const NS: &str = "crypto-widget-selftest";
        let credential = Credential {
            key: "test-key-0123456789".into(),
            secret: "test-secret-0123456789".into(),
        };

        save(NS, &credential).expect("save");
        assert_eq!(load(NS).as_ref(), Some(&credential));

        let status = status(NS);
        assert!(status.present);
        assert_eq!(status.masked_key.as_deref(), Some("test…6789"));

        delete(NS).expect("delete");
        assert!(load(NS).is_none());
        delete(NS).expect("deleting a missing credential is not an error");
    }
}
