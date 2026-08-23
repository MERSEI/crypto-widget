//! The seed phrase, and the accounts derived from it.
//!
//! This is the first thing in the app whose leak costs money rather than quota, so the rules
//! are stricter than for the futures keys:
//!
//! - The mnemonic lives in the OS credential store under its own namespace and is read only at
//!   the moment a transaction is signed. It is never written to `settings.json`, never logged,
//!   and never returned from a `#[tauri::command]` — with one deliberate exception,
//!   `reveal_phrase`, because a wallet nobody can back up is a wallet that loses everything on
//!   the first disk failure.
//! - Derivation is BIP-44 (`m/44'/60'/0'/0/index`), the path MetaMask and every hardware wallet
//!   use. Anything else would produce addresses no other client can restore, which turns "I
//!   wrote down my seed phrase" into a false sense of safety.

use alloy::signers::local::coins_bip39::{English, Mnemonic, Wordlist};
use alloy::signers::local::{MnemonicBuilder, PrivateKeySigner};
use rand::thread_rng;
use serde::Serialize;

/// Credential-store namespace. Separate from the futures namespaces so a lookup meant for an
/// exchange key can never come back holding a wallet seed.
pub const NAMESPACE: &str = "wallet-seed";

/// BIP-44 account path for Ethereum. Spelled out rather than assembled from constants, because
/// a typo here still yields a *valid* wallet — just at addresses the user's seed phrase does
/// not restore anywhere else, which is the worst way for this to fail.
fn derivation_path(index: u32) -> String {
    format!("m/44'/60'/0'/0/{index}")
}

/// What the renderer may know about the wallet: which address is active, never how to spend
/// from it. Same shape of guarantee as `secrets::CredentialStatus`, one layer up.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletStatus {
    pub initialized: bool,
    /// Checksummed address of the active account, when there is one.
    pub address: Option<String>,
    pub account_index: u32,
}

/// Generates a fresh 12-word English mnemonic from OS entropy.
///
/// 12 words rather than 24: 128 bits is already beyond brute force, and a phrase the user
/// actually transcribes correctly is worth more than twelve extra words they get wrong.
pub fn generate_phrase() -> Result<String, String> {
    let mnemonic = Mnemonic::<English>::new_with_count(&mut thread_rng(), 12)
        .map_err(|e| format!("could not generate a seed phrase: {e}"))?;
    Ok(mnemonic.to_phrase().trim().to_string())
}

/// Normalises and validates a phrase before it is stored.
///
/// The checksum check is the one that matters. A phrase with one word swapped for another
/// valid word passes a wordlist check and then opens a perfectly real, permanently empty
/// wallet — the user would see a zero balance and conclude their funds were stolen.
pub fn normalize_phrase(input: &str) -> Result<String, String> {
    let phrase = input
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");

    if phrase.is_empty() {
        return Err("the seed phrase is empty".into());
    }

    let words = phrase.split(' ').count();
    if !matches!(words, 12 | 15 | 18 | 21 | 24) {
        return Err(format!(
            "a seed phrase has 12, 15, 18, 21 or 24 words; {words} were given"
        ));
    }

    for word in phrase.split(' ') {
        if English::get_index(word).is_err() {
            return Err(format!("`{word}` is not a BIP-39 word"));
        }
    }

    Mnemonic::<English>::new_from_phrase(&phrase).map_err(|_| {
        "the seed phrase checksum does not match — check for a mistyped word".to_string()
    })?;

    Ok(phrase)
}

/// Stores a validated phrase, replacing whatever was there.
pub fn save_phrase(phrase: &str) -> Result<(), String> {
    let normalized = normalize_phrase(phrase)?;
    crate::secrets::save_raw(NAMESPACE, &normalized)
}

pub fn has_phrase() -> bool {
    crate::secrets::load_raw(NAMESPACE).is_some()
}

pub fn delete_phrase() -> Result<(), String> {
    crate::secrets::delete(NAMESPACE)
}

/// Reads the phrase back for display. The only path that hands seed material to the renderer,
/// and it exists for exactly one reason: letting the user write down their backup. Every
/// caller must be an explicit user action, never part of rendering a screen.
pub fn reveal_phrase() -> Result<String, String> {
    crate::secrets::load_raw(NAMESPACE).ok_or_else(|| "no wallet is set up yet".to_string())
}

/// Builds the signer for one account. This is the only place a private key exists, and it dies
/// with the returned value — nothing caches it.
pub fn signer_for(index: u32) -> Result<PrivateKeySigner, String> {
    let phrase =
        crate::secrets::load_raw(NAMESPACE).ok_or_else(|| "no wallet is set up yet".to_string())?;
    MnemonicBuilder::<English>::default()
        .phrase(phrase)
        .derivation_path(derivation_path(index))
        .map_err(|e| format!("bad derivation path: {e}"))?
        .build()
        .map_err(|e| format!("could not derive the account: {e}"))
}

/// The address of one account, with nothing attached that can spend from it.
pub fn address_for(index: u32) -> Result<String, String> {
    Ok(signer_for(index)?.address().to_checksum(None))
}

pub fn status(account_index: u32) -> WalletStatus {
    if !has_phrase() {
        return WalletStatus::default();
    }
    WalletStatus {
        initialized: true,
        address: address_for(account_index).ok(),
        account_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The canonical BIP-39 test vector. Its address at m/44'/60'/0'/0/0 is the one MetaMask
    /// shows for the same phrase — which is the entire point of pinning the path.
    const VECTOR: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    fn address_at(phrase: &str, index: u32) -> String {
        MnemonicBuilder::<English>::default()
            .phrase(phrase.to_string())
            .derivation_path(derivation_path(index))
            .unwrap()
            .build()
            .unwrap()
            .address()
            .to_checksum(None)
    }

    #[test]
    fn derives_the_same_address_as_every_other_bip44_wallet() {
        assert_eq!(
            address_at(&normalize_phrase(VECTOR).unwrap(), 0),
            "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
        );
    }

    #[test]
    fn the_account_index_selects_a_different_account() {
        let phrase = normalize_phrase(VECTOR).unwrap();
        assert_ne!(address_at(&phrase, 0), address_at(&phrase, 1));
        assert_eq!(
            address_at(&phrase, 1),
            "0x6Fac4D18c912343BF86fa7049364Dd4E424Ab9C0"
        );
    }

    #[test]
    fn a_phrase_is_normalised_before_it_is_judged() {
        let messy = "  ABANDON abandon\tabandon abandon abandon abandon abandon abandon abandon abandon abandon   about ";
        assert_eq!(normalize_phrase(messy).unwrap(), VECTOR);
    }

    #[test]
    fn a_mistyped_word_is_refused_rather_than_opening_an_empty_wallet() {
        // Not in the wordlist at all — the cheap check catches this one.
        let typo = VECTOR.replacen("abandon", "abandonn", 1);
        assert!(normalize_phrase(&typo).is_err());

        // `ability` *is* a valid BIP-39 word, so only the checksum stands between the user and
        // a real, empty wallet at addresses they have never funded.
        let plausible = VECTOR.replacen("abandon", "ability", 1);
        let err = normalize_phrase(&plausible).unwrap_err();
        assert!(err.contains("checksum"), "{err}");
    }

    #[test]
    fn word_counts_outside_bip39_are_refused() {
        assert!(normalize_phrase("abandon about").is_err());
        assert!(normalize_phrase("   ").is_err());
    }

    #[test]
    fn a_generated_phrase_round_trips_through_validation() {
        let phrase = generate_phrase().unwrap();
        assert_eq!(phrase.split(' ').count(), 12);
        assert_eq!(normalize_phrase(&phrase).unwrap(), phrase);
    }

    #[test]
    fn two_generated_phrases_are_not_the_same() {
        assert_ne!(generate_phrase().unwrap(), generate_phrase().unwrap());
    }
}
