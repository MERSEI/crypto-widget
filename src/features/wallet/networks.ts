/** Presets for the network switcher in wallet settings.
 *
 * Every preset here must support EIP-1559 fee estimation — `chain::quote` in the Rust backend
 * calls `estimate_eip1559_fees()` unconditionally, and a chain that only understands legacy gas
 * pricing (BSC, most notably) would fail every quote. That is why BSC is not in this list: adding
 * it needs a legacy-gas fallback on the Rust side first, not just an RPC URL here.
 *
 * RPC URLs are public, no-key endpoints, picked for the same reason as the Ethereum mainnet
 * default in `wallet/mod.rs`: some well-known "free" RPCs (`cloudflare-eth.com`,
 * `eth.llamarpc.com`) refuse requests from a desktop client, which looks like a dead network from
 * inside the app.
 */
export interface NetworkPreset {
  id: string;
  name: string;
  rpcUrl: string;
  chainId: number;
  nativeSymbol: string;
}

export const NETWORK_PRESETS: NetworkPreset[] = [
  {
    id: "ethereum",
    name: "Ethereum",
    rpcUrl: "https://ethereum-rpc.publicnode.com",
    chainId: 1,
    nativeSymbol: "ETH",
  },
  {
    id: "polygon",
    name: "Polygon",
    rpcUrl: "https://polygon-bor-rpc.publicnode.com",
    chainId: 137,
    // Polygon's native token was renamed from MATIC to POL in September 2024.
    nativeSymbol: "POL",
  },
  {
    id: "arbitrum",
    name: "Arbitrum One",
    rpcUrl: "https://arbitrum-one-rpc.publicnode.com",
    chainId: 42161,
    nativeSymbol: "ETH",
  },
  {
    id: "optimism",
    name: "Optimism",
    rpcUrl: "https://optimism-rpc.publicnode.com",
    chainId: 10,
    nativeSymbol: "ETH",
  },
  {
    id: "base",
    name: "Base",
    rpcUrl: "https://base-rpc.publicnode.com",
    chainId: 8453,
    nativeSymbol: "ETH",
  },
  {
    id: "sepolia",
    name: "Sepolia (testnet)",
    rpcUrl: "https://ethereum-sepolia-rpc.publicnode.com",
    chainId: 11155111,
    nativeSymbol: "ETH",
  },
];

/** Preset for a `{rpcUrl, chainId}` pair, or `null` when nothing matches — the escape hatch a
 * custom or since-changed network falls back to. Matched on both fields: a chain ID alone is not
 * enough to assume a specific RPC, and vice versa. */
export function presetFor(rpcUrl: string, chainId: number): NetworkPreset | null {
  return (
    NETWORK_PRESETS.find((p) => p.rpcUrl === rpcUrl && p.chainId === chainId) ?? null
  );
}

export const CUSTOM_NETWORK_ID = "custom";
