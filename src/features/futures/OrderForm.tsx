import { useEffect, useState } from "react";
import { commands } from "../../core/ipc/commands";
import { useFuturesStore } from "../../core/store/futures";
import type { OrderKind, OrderReceipt, OrderSide } from "../../types/futures";

function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function normalizeSymbol(value: string): string {
  return value.trim().toUpperCase();
}

/**
 * Order entry, plus per-symbol leverage and margin type — the three ways this terminal spends
 * money. Lives only in the standalone futures window: `FuturesPanel` (the 380px tab) stays
 * read-only, per its own doc comment.
 *
 * `FuturesHub::place_order` (and everything else here) refuses to run unless the active venue is
 * the testnet — this form does not re-check that, so a mainnet mode simply turns every submit
 * into that one refusal, surfaced as the request's error rather than the form pretending it
 * can't be reached at all.
 */
export function OrderForm() {
  const placeOrder = useFuturesStore((s) => s.placeOrder);
  const setLeverage = useFuturesStore((s) => s.setLeverage);
  const setMarginType = useFuturesStore((s) => s.setMarginType);

  const [confirmOrders, setConfirmOrders] = useState(true);
  useEffect(() => {
    void commands.getSettings().then((settings) => {
      setConfirmOrders(settings.futures.confirmOrders);
      setLeverageInput(String(settings.futures.defaultLeverage));
    });
    // Runs once on mount — the confirm/leverage default is a starting point for the fields
    // below, not a value this form keeps in sync with settings changed elsewhere.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const [symbol, setSymbol] = useState("");
  const [side, setSide] = useState<OrderSide>("buy");
  const [orderType, setOrderType] = useState<OrderKind>("market");
  const [quantity, setQuantity] = useState("");
  const [price, setPrice] = useState("");
  const [reduceOnly, setReduceOnly] = useState(false);
  const [confirmPending, setConfirmPending] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [receipt, setReceipt] = useState<OrderReceipt | null>(null);

  function edit<T>(setter: (value: T) => void) {
    return (value: T) => {
      setter(value);
      setConfirmPending(false);
      setError(null);
    };
  }

  async function submit() {
    if (confirmOrders && !confirmPending) {
      setConfirmPending(true);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await placeOrder(
        normalizeSymbol(symbol),
        side,
        orderType,
        Number(quantity),
        orderType === "limit" ? Number(price) : null,
        reduceOnly,
      );
      setReceipt(result);
      setConfirmPending(false);
      setQuantity("");
      setPrice("");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  const quantityValid = Number(quantity) > 0;
  const priceValid = orderType === "market" || Number(price) > 0;
  const canSubmit = normalizeSymbol(symbol).length > 0 && quantityValid && priceValid;

  // Leverage and margin type: per-symbol account settings, not part of an order. Applied
  // immediately on their own buttons rather than folded into the order form's submit, since a
  // trader adjusting them is usually not also placing an order in the same breath.
  const [leverageInput, setLeverageInput] = useState("5");
  const [leverageBusy, setLeverageBusy] = useState(false);
  const [leverageMessage, setLeverageMessage] = useState<string | null>(null);

  async function applyLeverage() {
    setLeverageBusy(true);
    setLeverageMessage(null);
    try {
      await setLeverage(normalizeSymbol(symbol), Number(leverageInput));
      setLeverageMessage(`Leverage set to ${leverageInput}x for ${normalizeSymbol(symbol)}`);
    } catch (e) {
      setLeverageMessage(errorMessage(e));
    } finally {
      setLeverageBusy(false);
    }
  }

  const [marginBusy, setMarginBusy] = useState(false);
  const [marginMessage, setMarginMessage] = useState<string | null>(null);

  async function applyMarginType(isolated: boolean) {
    setMarginBusy(true);
    setMarginMessage(null);
    try {
      await setMarginType(normalizeSymbol(symbol), isolated);
      setMarginMessage(`${normalizeSymbol(symbol)} set to ${isolated ? "isolated" : "cross"} margin`);
    } catch (e) {
      setMarginMessage(errorMessage(e));
    } finally {
      setMarginBusy(false);
    }
  }

  return (
    <section className="wallet-settings__group">
      <h3 className="wallet-settings__group-title">Order</h3>

      <label className="wallet-field">
        <span className="wallet-field__label">Symbol</span>
        <input
          className="wallet__input mono-nums"
          placeholder="BTCUSDT"
          spellCheck={false}
          autoComplete="off"
          value={symbol}
          onChange={(e) => edit(setSymbol)(e.target.value)}
        />
      </label>

      <div className="wallet-field wallet-field--inline">
        <label className="wallet-field">
          <span className="wallet-field__label">Side</span>
          <select value={side} onChange={(e) => edit(setSide)(e.target.value as OrderSide)}>
            <option value="buy">Buy / Long</option>
            <option value="sell">Sell / Short</option>
          </select>
        </label>
        <label className="wallet-field">
          <span className="wallet-field__label">Type</span>
          <select value={orderType} onChange={(e) => edit(setOrderType)(e.target.value as OrderKind)}>
            <option value="market">Market</option>
            <option value="limit">Limit</option>
          </select>
        </label>
      </div>

      <label className="wallet-field">
        <span className="wallet-field__label">Quantity</span>
        <input
          className="wallet__input mono-nums"
          inputMode="decimal"
          placeholder="0.0"
          value={quantity}
          onChange={(e) => edit(setQuantity)(e.target.value)}
        />
      </label>

      {orderType === "limit" && (
        <label className="wallet-field">
          <span className="wallet-field__label">Price</span>
          <input
            className="wallet__input mono-nums"
            inputMode="decimal"
            placeholder="0.0"
            value={price}
            onChange={(e) => edit(setPrice)(e.target.value)}
          />
        </label>
      )}

      <label className="wallet-field wallet-field--inline">
        <input
          type="checkbox"
          checked={reduceOnly}
          onChange={(e) => edit(setReduceOnly)(e.target.checked)}
        />
        <span>Reduce-only (never opens or adds to a position)</span>
      </label>

      {error && <div className="wallet__error">{error}</div>}

      {confirmPending && !error && (
        <div className="wallet-quote">
          <div className="wallet-quote__row">
            <span>About to send</span>
            <span className="mono-nums">
              {side === "buy" ? "BUY" : "SELL"} {quantity} {normalizeSymbol(symbol)} @{" "}
              {orderType === "market" ? "MARKET" : price}
            </span>
          </div>
        </div>
      )}

      {receipt && (
        <div className="wallet-quote">
          <div className="wallet-quote__row">
            <span>Order {receipt.status.toLowerCase()}</span>
            <span className="mono-nums">
              #{receipt.orderId} — {receipt.executedQty} @ {receipt.avgPrice || "pending fill"}
            </span>
          </div>
        </div>
      )}

      <div className="wallet-setup__actions">
        <button className="btn btn--primary" disabled={busy || !canSubmit} onClick={() => void submit()}>
          {confirmPending ? "Confirm and send" : "Place order"}
        </button>
        {confirmPending && (
          <button className="btn" disabled={busy} onClick={() => setConfirmPending(false)}>
            Cancel
          </button>
        )}
      </div>

      <h3 className="wallet-settings__group-title">Leverage &amp; margin — {normalizeSymbol(symbol) || "…"}</h3>
      <div className="wallet-field wallet-field--inline">
        <input
          className="wallet__input mono-nums"
          inputMode="numeric"
          style={{ maxWidth: "5rem" }}
          value={leverageInput}
          onChange={(e) => setLeverageInput(e.target.value)}
        />
        <button
          className="btn"
          disabled={leverageBusy || normalizeSymbol(symbol).length === 0}
          onClick={() => void applyLeverage()}
        >
          Set leverage
        </button>
      </div>
      {leverageMessage && <span className="wallet-field__hint">{leverageMessage}</span>}

      <div className="wallet-setup__actions">
        <button
          className="btn"
          disabled={marginBusy || normalizeSymbol(symbol).length === 0}
          onClick={() => void applyMarginType(true)}
        >
          Isolated
        </button>
        <button
          className="btn"
          disabled={marginBusy || normalizeSymbol(symbol).length === 0}
          onClick={() => void applyMarginType(false)}
        >
          Cross
        </button>
      </div>
      {marginMessage && <span className="wallet-field__hint">{marginMessage}</span>}
    </section>
  );
}
