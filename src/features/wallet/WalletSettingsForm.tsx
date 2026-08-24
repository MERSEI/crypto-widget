import { useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useWalletStore, walletErrorMessage } from "../../core/store/wallet";
import { CUSTOM_NETWORK_ID, NETWORK_PRESETS, presetFor } from "./networks";
import { SeedPhrase } from "./SeedPhrase";

/**
 * Network, account, history key, and the two irreversible actions.
 *
 * "Reveal" and "Remove wallet" both sit behind a typed confirmation rather than a click: one
 * puts the only copy of the funds on screen, the other deletes it from this machine, and
 * neither belongs a stray click away from a settings form.
 */
export function WalletSettingsForm() {
  const state = useWalletStore((s) => s.state);
  const apply = useWalletStore((s) => s.apply);
  const hydrate = useWalletStore((s) => s.hydrate);
  const refreshBalances = useWalletStore((s) => s.refreshBalances);

  const [rpcUrl, setRpcUrl] = useState(state?.settings.rpcUrl ?? "");
  const [chainId, setChainId] = useState(String(state?.settings.chainId ?? 1));
  const [nativeSymbol, setNativeSymbol] = useState(state?.settings.nativeSymbol ?? "ETH");
  const [networkId, setNetworkId] = useState(
    () =>
      presetFor(state?.settings.rpcUrl ?? "", state?.settings.chainId ?? 1)?.id ??
      CUSTOM_NETWORK_ID,
  );
  const [accountIndex, setAccountIndex] = useState(String(state?.settings.accountIndex ?? 0));
  const [apiKey, setApiKey] = useState("");
  const [phrase, setPhrase] = useState<string | null>(null);
  const [confirmRemove, setConfirmRemove] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!state) return null;

  function selectNetwork(id: string) {
    setNetworkId(id);
    if (id === CUSTOM_NETWORK_ID) return;
    const preset = NETWORK_PRESETS.find((p) => p.id === id);
    if (!preset) return;
    setRpcUrl(preset.rpcUrl);
    setChainId(String(preset.chainId));
    setNativeSymbol(preset.nativeSymbol);
  }

  async function run(action: () => Promise<void>) {
    setBusy(true);
    setError(null);
    try {
      await action();
    } catch (e) {
      setError(walletErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="wallet-settings">
      <header className="wallet-section__header">
        <h2 className="wallet-section__title">Settings</h2>
      </header>

      {error && <div className="wallet__error">{error}</div>}

      <div className="wallet-settings__group">
        <h3 className="wallet-settings__group-title">Network</h3>
        <label className="wallet-field">
          <span className="wallet-field__label">Chain</span>
          <select
            className="wallet__input"
            value={networkId}
            onChange={(e) => selectNetwork(e.target.value)}
          >
            {NETWORK_PRESETS.map((preset) => (
              <option key={preset.id} value={preset.id}>
                {preset.name}
              </option>
            ))}
            <option value={CUSTOM_NETWORK_ID}>Custom</option>
          </select>
          <span className="wallet-field__hint">
            The wallet works on any EVM chain — this list is a shortcut for the common ones. Pick
            Custom to point at a different RPC by hand.
          </span>
        </label>

        {networkId === CUSTOM_NETWORK_ID && (
          <>
            <label className="wallet-field">
              <span className="wallet-field__label">RPC URL</span>
              <input
                className="wallet__input"
                spellCheck={false}
                autoComplete="off"
                value={rpcUrl}
                onChange={(e) => setRpcUrl(e.target.value)}
              />
            </label>
            <label className="wallet-field">
              <span className="wallet-field__label">Chain ID</span>
              <input
                className="wallet__input mono-nums"
                inputMode="numeric"
                value={chainId}
                onChange={(e) => setChainId(e.target.value)}
              />
              <span className="wallet-field__hint">
                The chain the RPC serves, and the one the history is read for — a mismatch shows
                an empty wallet, not an error.
              </span>
            </label>
            <label className="wallet-field">
              <span className="wallet-field__label">Currency symbol</span>
              <input
                className="wallet__input"
                spellCheck={false}
                autoComplete="off"
                placeholder="ETH"
                value={nativeSymbol}
                onChange={(e) => setNativeSymbol(e.target.value)}
              />
              <span className="wallet-field__hint">
                Only cosmetic — how the native balance and fees are labelled. This chain must
                still support EIP-1559 fees, or every quote will fail.
              </span>
            </label>
          </>
        )}

        <button
          className="btn"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              apply(await commands.setWalletNetwork(rpcUrl, Number(chainId), nativeSymbol));
              await refreshBalances();
            })
          }
        >
          Save network
        </button>
      </div>

      <div className="wallet-settings__group">
        <h3 className="wallet-settings__group-title">Account</h3>
        <label className="wallet-field">
          <span className="wallet-field__label">BIP-44 index</span>
          <input
            className="wallet__input mono-nums"
            inputMode="numeric"
            value={accountIndex}
            onChange={(e) => setAccountIndex(e.target.value)}
          />
          <span className="wallet-field__hint mono-nums">
            {state.status.address ?? "no address"}
          </span>
        </label>
        <button
          className="btn"
          disabled={busy}
          onClick={() =>
            void run(async () => {
              apply(await commands.setWalletAccount(Number(accountIndex)));
              await refreshBalances();
            })
          }
        >
          Switch account
        </button>
      </div>

      <div className="wallet-settings__group">
        <h3 className="wallet-settings__group-title">History</h3>
        <p className="wallet-field__hint">
          Transaction history comes from Etherscan and needs a free API key. Everything else in
          the wallet works without one.
        </p>
        <label className="wallet-field">
          <span className="wallet-field__label">Etherscan API key</span>
          <input
            className="wallet__input"
            type="password"
            spellCheck={false}
            autoComplete="off"
            placeholder={state.etherscan.maskedKey ?? "not set"}
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
          />
        </label>
        <div className="wallet-setup__actions">
          <button
            className="btn"
            disabled={busy || apiKey.trim().length === 0}
            onClick={() =>
              void run(async () => {
                apply(await commands.setEtherscanKey(apiKey));
                setApiKey("");
              })
            }
          >
            Save key
          </button>
          {state.etherscan.present && (
            <button
              className="btn"
              disabled={busy}
              onClick={() => void run(async () => apply(await commands.clearEtherscanKey()))}
            >
              Remove key
            </button>
          )}
        </div>
      </div>

      <div className="wallet-settings__group">
        <h3 className="wallet-settings__group-title">Price pill</h3>
        <label className="wallet-field wallet-field--inline">
          <input
            type="checkbox"
            checked={state.settings.widgetEnabled}
            disabled={busy}
            onChange={(e) =>
              void run(async () =>
                apply(await commands.setWalletWidgetEnabled(e.target.checked)),
              )
            }
          />
          <span>Keep the docked price pill running</span>
        </label>
      </div>

      <div className="wallet-settings__group wallet-settings__group--danger">
        <h3 className="wallet-settings__group-title">Backup and removal</h3>

        {phrase ? (
          <>
            <SeedPhrase phrase={phrase} />
            <button className="btn" onClick={() => setPhrase(null)}>
              Hide
            </button>
          </>
        ) : (
          <button
            className="btn"
            disabled={busy}
            onClick={() => void run(async () => setPhrase(await commands.revealSeedPhrase()))}
          >
            Reveal seed phrase
          </button>
        )}

        <label className="wallet-field">
          <span className="wallet-field__label">
            Remove this wallet — type REMOVE to confirm
          </span>
          <input
            className="wallet__input"
            spellCheck={false}
            autoComplete="off"
            value={confirmRemove}
            onChange={(e) => setConfirmRemove(e.target.value)}
          />
          <span className="wallet-field__hint">
            Deletes the seed from the Windows Credential Manager. Without the phrase written
            down, the funds are gone with it.
          </span>
        </label>
        <button
          className="btn btn--danger"
          disabled={busy || confirmRemove !== "REMOVE"}
          onClick={() =>
            void run(async () => {
              await commands.forgetWallet();
              setConfirmRemove("");
              setPhrase(null);
              await hydrate();
            })
          }
        >
          Remove wallet
        </button>
      </div>
    </section>
  );
}
