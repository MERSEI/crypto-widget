//! Everything that talks to an Ethereum node: balances, fee estimates, and the one path that
//! spends money.
//!
//! Two decisions govern this module.
//!
//! **Amounts cross the IPC boundary as strings, never as numbers.** A balance of
//! 1234.567891234567891 wei-scaled does not survive a round trip through a JS `number`, and
//! silently losing the low digits of a transfer amount is the kind of bug that only shows up
//! once real money is moving. Every amount here is either base units (a decimal string) or a
//! pre-formatted display string produced on this side.
//!
//! **Token decimals are always read from the contract.** Assuming 6 or 18 and being wrong
//! scales a transfer by a factor of a trillion in either direction.

use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::utils::{format_units, parse_units};
use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::rpc::types::TransactionRequest;
use alloy::sol;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function decimals() external view returns (uint8);
        function symbol() external view returns (string);
        function transfer(address to, uint256 amount) external returns (bool);
    }
}

/// One line of the portfolio. `raw` is the exact base-unit value; `amount` is the same number
/// formatted for display. The UI shows `amount` and computes with `raw`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetBalance {
    /// `None` for the native currency, the contract address for an ERC-20.
    pub contract: Option<String>,
    pub symbol: String,
    pub decimals: u8,
    pub raw: String,
    pub amount: String,
    /// Set when this one row could not be read — a token contract that does not answer, most
    /// often. The row still renders, because dropping it silently would look like the token was
    /// never added; what it must not do is show a confident `0`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What a transfer will cost and whether it can go through, worked out *before* anything is
/// signed. The user sees this and approves it; nothing is broadcast until they do.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeQuote {
    pub gas_limit: String,
    pub max_fee_per_gas: String,
    pub max_priority_fee_per_gas: String,
    /// gas_limit × max_fee_per_gas, in wei.
    pub max_cost_wei: String,
    /// The same figure in ETH, for display.
    pub max_cost_eth: String,
    /// False when the account cannot cover amount + fee. The UI refuses to send on this.
    pub affordable: bool,
    /// Present when `affordable` is false: what exactly is short.
    pub shortfall: Option<String>,
}

/// Parses an address, enforcing the EIP-55 checksum whenever the input carries one.
///
/// `Address::from_str` does **not** check the checksum — it accepts any 40 hex characters. On a
/// send path that is the difference between "refused" and "funds delivered to an address nobody
/// controls", because a single mistyped character in a pasted address still parses. So:
/// mixed-case input is verified against its checksum, while all-lower or all-upper input has no
/// checksum to verify and is taken as written (that is what the standard says, and it is how
/// every explorer's "copy address" behaves).
pub fn parse_address(value: &str) -> Result<Address, String> {
    let trimmed = value.trim();
    let invalid = || format!("`{trimmed}` is not a valid Ethereum address");

    let body = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X"));
    let Some(body) = body else {
        return Err(invalid());
    };
    if body.len() != 40 || !body.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid());
    }

    let has_upper = body.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = body.chars().any(|c| c.is_ascii_lowercase());

    if has_upper && has_lower {
        return Address::parse_checksummed(trimmed, None).map_err(|_| {
            format!("`{trimmed}` fails its checksum — one character is likely mistyped")
        });
    }

    Address::from_str(trimmed).map_err(|_| invalid())
}

fn http_provider(rpc_url: &str) -> Result<impl Provider, String> {
    let url = rpc_url
        .parse()
        .map_err(|_| format!("`{rpc_url}` is not a valid RPC URL"))?;
    Ok(ProviderBuilder::new().connect_http(url))
}

/// Reads the native balance of an address.
pub async fn native_balance(rpc_url: &str, address: Address) -> Result<AssetBalance, String> {
    let provider = http_provider(rpc_url)?;
    let raw = provider
        .get_balance(address)
        .await
        .map_err(|e| format!("could not read the balance: {e}"))?;
    Ok(AssetBalance {
        contract: None,
        symbol: "ETH".into(),
        decimals: 18,
        raw: raw.to_string(),
        amount: format_units(raw, 18).map_err(|e| e.to_string())?,
        error: None,
    })
}

/// Reads one ERC-20 balance, along with the symbol and decimals the contract reports.
///
/// The metadata is read rather than trusted from settings: a token entry the user typed in by
/// hand can carry the wrong decimals, and every figure derived from it would then be wrong by
/// orders of magnitude.
pub async fn token_balance(
    rpc_url: &str,
    owner: Address,
    contract: Address,
) -> Result<AssetBalance, String> {
    let provider = http_provider(rpc_url)?;
    let erc20 = IERC20::new(contract, &provider);

    let raw = erc20
        .balanceOf(owner)
        .call()
        .await
        .map_err(|e| format!("could not read the token balance: {e}"))?;
    let decimals = erc20
        .decimals()
        .call()
        .await
        .map_err(|e| format!("could not read the token decimals: {e}"))?;
    let symbol = erc20
        .symbol()
        .call()
        .await
        .unwrap_or_else(|_| "TOKEN".to_string());

    Ok(AssetBalance {
        contract: Some(contract.to_checksum(None)),
        symbol,
        decimals,
        raw: raw.to_string(),
        amount: format_units(raw, decimals).map_err(|e| e.to_string())?,
        error: None,
    })
}

/// Converts a decimal amount typed by a human into base units for a token with `decimals`.
///
/// Refuses more precision than the token has instead of truncating: "0.0000001" of a 6-decimal
/// token is not 0, and silently rounding it to 0 would send an empty transfer that still costs
/// gas.
pub fn to_base_units(amount: &str, decimals: u8) -> Result<U256, String> {
    let trimmed = amount.trim();
    if trimmed.is_empty() {
        return Err("enter an amount".into());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.')
        || trimmed.matches('.').count() > 1
    {
        return Err("the amount must be a plain decimal number, e.g. 12.50".into());
    }
    if let Some((_, frac)) = trimmed.split_once('.') {
        if frac.len() > decimals as usize {
            return Err(format!(
                "this asset has {decimals} decimals; {} were given",
                frac.len()
            ));
        }
    }
    let units = parse_units(trimmed, decimals)
        .map_err(|e| format!("could not read that amount: {e}"))?
        .into();
    if units == U256::ZERO {
        return Err("the amount must be greater than zero".into());
    }
    Ok(units)
}

pub fn format_amount(raw: U256, decimals: u8) -> String {
    format_units(raw, decimals).unwrap_or_else(|_| raw.to_string())
}

/// Builds the transaction a transfer would send, without signing it.
///
/// Split out from `send` so the fee quote the user approves is derived from the very same
/// request that is later broadcast — quoting one transaction and sending another is how a
/// "confirm 0.002 ETH fee" dialog ends up authorising something else.
async fn build_request(
    rpc_url: &str,
    from: Address,
    to: Address,
    amount: &str,
    token: Option<Address>,
) -> Result<(TransactionRequest, U256), String> {
    let provider = http_provider(rpc_url)?;

    match token {
        None => {
            let value = to_base_units(amount, 18)?;
            let request = TransactionRequest::default()
                .with_from(from)
                .with_to(to)
                .with_value(value);
            Ok((request, value))
        }
        Some(contract) => {
            let erc20 = IERC20::new(contract, &provider);
            let decimals = erc20
                .decimals()
                .call()
                .await
                .map_err(|e| format!("could not read the token decimals: {e}"))?;
            let units = to_base_units(amount, decimals)?;

            let balance = erc20
                .balanceOf(from)
                .call()
                .await
                .map_err(|e| format!("could not read the token balance: {e}"))?;
            if balance < units {
                return Err(format!(
                    "not enough tokens: {} available",
                    format_amount(balance, decimals)
                ));
            }

            let call = erc20.transfer(to, units);
            let request = TransactionRequest::default()
                .with_from(from)
                .with_to(contract)
                .with_input(call.calldata().clone());
            // An ERC-20 transfer moves no ether, so the value that has to be *covered* is zero;
            // only the fee comes out of the native balance.
            Ok((request, U256::ZERO))
        }
    }
}

/// Estimates gas and fees for a transfer and checks the account can cover it.
pub async fn quote(
    rpc_url: &str,
    from: Address,
    to: Address,
    amount: &str,
    token: Option<Address>,
) -> Result<FeeQuote, String> {
    let provider = http_provider(rpc_url)?;
    let (request, value) = build_request(rpc_url, from, to, amount, token).await?;

    let gas_limit = provider
        .estimate_gas(request)
        .await
        .map_err(|e| format!("the network rejected this transfer: {e}"))?;

    let fees = provider
        .estimate_eip1559_fees()
        .await
        .map_err(|e| format!("could not read current gas prices: {e}"))?;

    let max_cost = U256::from(gas_limit) * U256::from(fees.max_fee_per_gas);
    let native = provider
        .get_balance(from)
        .await
        .map_err(|e| format!("could not read the balance: {e}"))?;

    let required = max_cost + value;
    let affordable = native >= required;

    Ok(FeeQuote {
        gas_limit: gas_limit.to_string(),
        max_fee_per_gas: fees.max_fee_per_gas.to_string(),
        max_priority_fee_per_gas: fees.max_priority_fee_per_gas.to_string(),
        max_cost_wei: max_cost.to_string(),
        max_cost_eth: format_amount(max_cost, 18),
        affordable,
        shortfall: if affordable {
            None
        } else {
            Some(format!(
                "{} ETH short of the amount plus the maximum fee",
                format_amount(required - native, 18)
            ))
        },
    })
}

/// Signs and broadcasts a transfer, returning the transaction hash.
///
/// Returns as soon as the node accepts the transaction rather than waiting for a receipt: a
/// mainnet inclusion can take minutes, and a UI that blocks on it looks hung and invites the
/// user to click send a second time.
pub async fn send(
    rpc_url: &str,
    signer: alloy::signers::local::PrivateKeySigner,
    to: Address,
    amount: &str,
    token: Option<Address>,
) -> Result<String, String> {
    let from = signer.address();
    let (request, _) = build_request(rpc_url, from, to, amount, token).await?;

    let url = rpc_url
        .parse()
        .map_err(|_| format!("`{rpc_url}` is not a valid RPC URL"))?;
    let provider = ProviderBuilder::new()
        .wallet(EthereumWallet::from(signer))
        .connect_http(url);

    let pending = provider
        .send_transaction(request)
        .await
        .map_err(|e| format!("the transfer was refused: {e}"))?;

    Ok(pending.tx_hash().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mistyped_checksummed_address_is_refused() {
        // Valid address, one character changed: the checksum no longer matches.
        assert!(parse_address("0x9858EfFD232B4033E47d90003D41EC34EcaEda94").is_ok());
        assert!(parse_address("0x9858EfFD232B4033E47d90003D41EC34EcaEda95").is_err());
        // All-lowercase carries no checksum, so it is accepted as written.
        assert!(parse_address("0x9858effd232b4033e47d90003d41ec34ecaeda94").is_ok());
        // As is all-uppercase, which is how some older tools print addresses.
        assert!(parse_address("0x9858EFFD232B4033E47D90003D41EC34ECAEDA94").is_ok());
    }

    #[test]
    fn surrounding_whitespace_from_a_paste_is_tolerated() {
        assert_eq!(
            parse_address("  0x9858EfFD232B4033E47d90003D41EC34EcaEda94\n")
                .unwrap()
                .to_checksum(None),
            "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
        );
    }

    #[test]
    fn addresses_that_are_not_addresses_are_refused() {
        for bad in ["", "0x", "not an address", "9858EfFD232B4033E47d90003D41EC34EcaEda94x"] {
            assert!(parse_address(bad).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn amounts_scale_by_the_assets_own_decimals() {
        assert_eq!(to_base_units("1", 18).unwrap(), U256::from(10u128.pow(18)));
        assert_eq!(to_base_units("1", 6).unwrap(), U256::from(1_000_000u64));
        assert_eq!(to_base_units("0.5", 6).unwrap(), U256::from(500_000u64));
    }

    #[test]
    fn more_precision_than_the_asset_has_is_refused_not_truncated() {
        // 0.0000001 USDC is not 0 USDC — rounding it away would send an empty transfer that
        // still burns gas.
        let err = to_base_units("0.0000001", 6).unwrap_err();
        assert!(err.contains("6 decimals"), "{err}");
    }

    #[test]
    fn zero_and_malformed_amounts_are_refused() {
        for bad in ["", "0", "0.0", "-1", "1e18", "abc", "1.2.3", "1,5"] {
            assert!(to_base_units(bad, 18).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn formatting_survives_the_full_width_of_a_uint256_balance() {
        let raw = U256::from(123_456_789_012_345_678u128);
        assert_eq!(format_amount(raw, 18), "0.123456789012345678");
    }
}
