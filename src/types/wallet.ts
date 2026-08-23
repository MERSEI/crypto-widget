/** Mirrors the Rust types in `src-tauri/src/wallet/` — change them in pairs.
 *
 *  Every amount is a string, and none of them may be turned into a `number` before it is shown.
 *  A wei-scaled balance runs to 18 digits past the point; `Number` silently drops the tail, and
 *  a transfer amount that quietly loses its last digits is the kind of bug that only surfaces
 *  once real money is moving. The backend sends both the exact base-unit value (`raw`) and a
 *  pre-formatted one (`amount`) so the UI never has to do the arithmetic itself. */

/** Masked view of a stored secret — the same shape the futures keys use. */
export interface CredentialStatus {
  present: boolean;
  maskedKey: string | null;
}

export interface WalletStatusInfo {
  initialized: boolean;
  /** Checksummed address of the active account, or null when no wallet is set up. */
  address: string | null;
  accountIndex: number;
}

export interface TokenEntry {
  address: string;
  symbol: string;
  decimals: number;
}

/** Non-secret half of the wallet. The seed phrase lives in the OS credential store and reaches
 *  the renderer only through an explicit reveal. */
export interface WalletSettings {
  rpcUrl: string;
  chainId: number;
  accountIndex: number;
  tokens: TokenEntry[];
  widgetEnabled: boolean;
}

export interface WalletState {
  status: WalletStatusInfo;
  settings: WalletSettings;
  etherscan: CredentialStatus;
}

export interface NewWallet {
  phrase: string;
  state: WalletState;
}

export interface AssetBalance {
  /** Null for the native currency, the contract address for an ERC-20. */
  contract: string | null;
  symbol: string;
  decimals: number;
  /** Exact base units. */
  raw: string;
  /** The same figure, formatted for display. */
  amount: string;
  /** Present when this one row could not be read — the row still renders, marked. */
  error?: string;
}

export interface FeeQuote {
  gasLimit: string;
  maxFeePerGas: string;
  maxPriorityFeePerGas: string;
  /** gasLimit × maxFeePerGas, in wei. Hand this back to `sendWalletTransfer` as the approved
   *  figure — the backend re-quotes and refuses if the real cost has run away from it. */
  maxCostWei: string;
  maxCostEth: string;
  affordable: boolean;
  shortfall: string | null;
}

export type TransferDirection = "in" | "out";
export type TransferAsset = "native" | "token";

export interface Transfer {
  hash: string;
  from: string;
  /** Null for a contract creation. */
  to: string | null;
  amount: string;
  /** Unix milliseconds. */
  timestamp: number;
  direction: TransferDirection;
  asset: TransferAsset;
  symbol: string;
  /** True when the transaction reverted: it happened, but it moved no value. */
  failed: boolean;
}
