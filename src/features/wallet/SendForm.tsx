import { useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useWalletStore, walletErrorMessage } from "../../core/store/wallet";
import type { FeeQuote } from "../../types/wallet";

/**
 * The send path: fill in, review, confirm.
 *
 * Two steps rather than one, and the quote is state that any edit destroys. What the user
 * approves is `quote.maxCostWei`, and that exact figure goes back to the backend, which
 * re-quotes and refuses if the real cost has drifted past it. So a stale quote on screen can
 * only ever cause a refusal — never a transfer at a fee nobody agreed to.
 *
 * The amount is never parsed into a `number` here. It travels as the string the user typed and
 * is scaled in Rust against the decimals read from the contract.
 */
export function SendForm() {
  const balances = useWalletStore((s) => s.balances);
  const refreshBalances = useWalletStore((s) => s.refreshBalances);
  const refreshHistory = useWalletStore((s) => s.refreshHistory);

  const [contract, setContract] = useState<string>("");
  const [to, setTo] = useState("");
  const [amount, setAmount] = useState("");
  const [quote, setQuote] = useState<FeeQuote | null>(null);
  const [hash, setHash] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const asset = balances.find((b) => (b.contract ?? "") === contract);

  /** Anything that changes what would be sent invalidates the approval that was given for it. */
  function edit<T>(setter: (value: T) => void) {
    return (value: T) => {
      setter(value);
      setQuote(null);
      setHash(null);
      setError(null);
    };
  }

  async function review() {
    setBusy(true);
    setError(null);
    try {
      setQuote(await commands.quoteWalletTransfer(to, amount, contract || null));
    } catch (e) {
      setError(walletErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function send() {
    if (!quote) return;
    setBusy(true);
    setError(null);
    try {
      const txHash = await commands.sendWalletTransfer(
        to,
        amount,
        contract || null,
        quote.maxCostWei,
      );
      setHash(txHash);
      setQuote(null);
      setTo("");
      setAmount("");
      await refreshBalances();
      // The transfer is accepted, not yet mined, so it will not be in the history for a block
      // or two — refreshing anyway costs one request and covers the case where it is.
      void refreshHistory();
    } catch (e) {
      setError(walletErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="wallet-send">
      <header className="wallet-section__header">
        <h2 className="wallet-section__title">Send</h2>
      </header>

      <label className="wallet-field">
        <span className="wallet-field__label">Asset</span>
        <select value={contract} onChange={(e) => edit(setContract)(e.target.value)}>
          <option value="">ETH</option>
          {balances
            .filter((b) => b.contract)
            .map((b) => (
              <option key={b.contract} value={b.contract as string}>
                {b.symbol}
              </option>
            ))}
        </select>
      </label>

      <label className="wallet-field">
        <span className="wallet-field__label">To</span>
        <input
          className="wallet__input mono-nums"
          placeholder="0x…"
          spellCheck={false}
          autoComplete="off"
          value={to}
          onChange={(e) => edit(setTo)(e.target.value)}
        />
      </label>

      <label className="wallet-field">
        <span className="wallet-field__label">Amount</span>
        <input
          className="wallet__input mono-nums"
          placeholder="0.0"
          inputMode="decimal"
          spellCheck={false}
          autoComplete="off"
          value={amount}
          onChange={(e) => edit(setAmount)(e.target.value)}
        />
        {asset && (
          <span className="wallet-field__hint mono-nums" title={asset.amount}>
            {asset.amount} {asset.symbol} available
          </span>
        )}
      </label>

      {error && <div className="wallet__error">{error}</div>}

      {quote && (
        <div className={`wallet-quote ${quote.affordable ? "" : "wallet-quote--short"}`}>
          <div className="wallet-quote__row">
            <span>Gas limit</span>
            <span className="mono-nums">{quote.gasLimit}</span>
          </div>
          <div className="wallet-quote__row">
            <span>Maximum fee</span>
            <span className="mono-nums">{quote.maxCostEth} ETH</span>
          </div>
          {!quote.affordable && quote.shortfall && (
            <div className="wallet-quote__row wallet-quote__shortfall">
              <span>Short by</span>
              <span className="mono-nums">{quote.shortfall}</span>
            </div>
          )}
        </div>
      )}

      {hash && (
        <div className="wallet-send__sent">
          <span>Broadcast</span>
          <code className="mono-nums wallet-send__hash" title={hash}>
            {hash}
          </code>
          <span className="wallet-field__hint">
            Accepted by the node — it appears in the history once it is mined.
          </span>
        </div>
      )}

      <div className="wallet-setup__actions">
        {quote ? (
          <button
            className="btn btn--primary"
            disabled={busy || !quote.affordable}
            onClick={() => void send()}
          >
            Confirm and send
          </button>
        ) : (
          <button
            className="btn btn--primary"
            disabled={busy || to.trim() === "" || amount.trim() === ""}
            onClick={() => void review()}
          >
            Review transfer
          </button>
        )}
        {quote && (
          <button className="btn" disabled={busy} onClick={() => setQuote(null)}>
            Cancel
          </button>
        )}
      </div>
    </section>
  );
}
