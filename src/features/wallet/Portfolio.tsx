import { useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useWalletStore, walletErrorMessage } from "../../core/store/wallet";
import type { AssetBalance } from "../../types/wallet";

/** Trims a formatted amount for display without touching the value the backend sent.
 *
 *  `0.123456789012345678` is noise at a glance; the full figure stays in the title attribute so
 *  nothing is actually hidden. Cutting at 8 places is a display choice made on a string — the
 *  number is never parsed, because parsing it is what loses the tail. */
function short(amount: string): string {
  const [whole, frac] = amount.split(".");
  if (!frac || frac.length <= 8) return amount;
  return `${whole}.${frac.slice(0, 8)}…`;
}

function BalanceRow({ balance, onRemove }: { balance: AssetBalance; onRemove?: () => void }) {
  return (
    <li className={`wallet-balance ${balance.error ? "wallet-balance--failed" : ""}`}>
      <span className="wallet-balance__symbol">{balance.symbol}</span>
      <span className="wallet-balance__amount mono-nums" title={balance.amount}>
        {balance.error ? "—" : short(balance.amount)}
      </span>
      {balance.error && (
        <span className="wallet-balance__error" title={balance.error}>
          unreadable
        </span>
      )}
      {onRemove && (
        <button className="icon-btn" title="Remove this token" onClick={onRemove}>
          ×
        </button>
      )}
    </li>
  );
}

/** Balances for the active account: the native currency first, then one row per token. */
export function Portfolio() {
  const state = useWalletStore((s) => s.state);
  const balances = useWalletStore((s) => s.balances);
  const loading = useWalletStore((s) => s.balancesLoading);
  const error = useWalletStore((s) => s.balancesError);
  const apply = useWalletStore((s) => s.apply);
  const refreshBalances = useWalletStore((s) => s.refreshBalances);

  const [contract, setContract] = useState("");
  const [busy, setBusy] = useState(false);
  const [tokenError, setTokenError] = useState<string | null>(null);

  async function addToken() {
    setBusy(true);
    setTokenError(null);
    try {
      // The backend reads the symbol and decimals off the contract, so nothing typed here can
      // misprice a balance — a wrong address fails outright instead.
      apply(await commands.addWalletToken(contract));
      setContract("");
      await refreshBalances();
    } catch (e) {
      setTokenError(walletErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeToken(address: string) {
    apply(await commands.removeWalletToken(address));
    await refreshBalances();
  }

  return (
    <section className="wallet-portfolio">
      <header className="wallet-section__header">
        <h2 className="wallet-section__title">Balances</h2>
        <div className="futures__toolbar-spacer" />
        <button
          className="icon-btn"
          title="Refresh balances"
          disabled={loading}
          onClick={() => void refreshBalances()}
        >
          ↻
        </button>
      </header>

      {error && <div className="wallet__error">{error}</div>}

      {balances.length === 0 && !loading && !error ? (
        <p className="wallet__empty">Nothing loaded yet.</p>
      ) : (
        <ul className="wallet-balances">
          {balances.map((balance) => (
            <BalanceRow
              key={balance.contract ?? "native"}
              balance={balance}
              onRemove={
                balance.contract
                  ? () => void removeToken(balance.contract as string)
                  : undefined
              }
            />
          ))}
        </ul>
      )}

      <div className="wallet-portfolio__add">
        <input
          className="wallet__input"
          placeholder="ERC-20 contract address"
          spellCheck={false}
          autoComplete="off"
          value={contract}
          onChange={(e) => setContract(e.target.value)}
        />
        <button
          className="btn"
          disabled={busy || contract.trim().length === 0 || !state?.status.initialized}
          onClick={() => void addToken()}
        >
          Add token
        </button>
      </div>
      {tokenError && <div className="wallet__error">{tokenError}</div>}
    </section>
  );
}
