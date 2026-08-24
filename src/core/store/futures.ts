import { create } from "zustand";
import { commands } from "../ipc/commands";
import type {
  FuturesKeyStatus,
  FuturesState,
  OrderKind,
  OrderRecord,
  OrderReceipt,
  OrderSide,
  VenueMode,
} from "../../types/futures";

/**
 * Futures account mirror.
 *
 * Rust polls the exchange and emits `futures`; this store holds the last state it was handed.
 * Nothing here derives a number — PnL, ROE and the price move all arrive computed, so the panel
 * and the backend can never disagree about what a position is worth.
 */
interface FuturesStoreState {
  state: FuturesState | null;
  keys: FuturesKeyStatus | null;
  /** Result of the last "Test connection", cleared when the venue or key changes. */
  testResult: string | null;
  /** Symbol the order-history tab last loaded, or is loading — Binance has no "every symbol"
   *  history endpoint, so this is always one specific symbol. */
  orderHistorySymbol: string;
  orderHistory: OrderRecord[];
  orderHistoryLoading: boolean;
  orderHistoryError: string | null;
  hydrate: () => Promise<void>;
  apply: (state: FuturesState) => void;
  setEnabled: (enabled: boolean) => Promise<void>;
  setVenue: (venue: VenueMode) => Promise<void>;
  saveKeys: (venue: VenueMode, apiKey: string, apiSecret: string) => Promise<void>;
  clearKeys: (venue: VenueMode) => Promise<void>;
  refresh: () => Promise<void>;
  testConnection: () => Promise<void>;
  /** These four throw on failure rather than swallowing it into state — an order form needs to
   *  know its submit failed, not infer it from a snapshot that stayed unchanged. */
  placeOrder: (
    symbol: string,
    side: OrderSide,
    orderType: OrderKind,
    quantity: number,
    price: number | null,
    reduceOnly: boolean,
  ) => Promise<OrderReceipt>;
  closePosition: (symbol: string, side: OrderSide, quantity: number | null) => Promise<OrderReceipt>;
  cancelOrder: (symbol: string, orderId: number) => Promise<void>;
  setLeverage: (symbol: string, leverage: number) => Promise<void>;
  setMarginType: (symbol: string, isolated: boolean) => Promise<void>;
  loadOrderHistory: (symbol: string) => Promise<void>;
}

export const useFuturesStore = create<FuturesStoreState>((set, get) => ({
  state: null,
  keys: null,
  testResult: null,
  orderHistorySymbol: "",
  orderHistory: [],
  orderHistoryLoading: false,
  orderHistoryError: null,

  hydrate: async () => {
    const [state, keys] = await Promise.all([commands.getFuturesState(), commands.getFuturesKeys()]);
    set({ state, keys });
  },

  apply: (state) => set({ state }),

  setEnabled: async (enabled) => {
    set({ state: await commands.setFuturesEnabled(enabled) });
  },

  setVenue: async (venue) => {
    set({ state: await commands.setFuturesVenue(venue), testResult: null });
  },

  saveKeys: async (venue, apiKey, apiSecret) => {
    const keys = await commands.setFuturesKeys(venue, apiKey, apiSecret);
    set({ keys, testResult: null, state: await commands.getFuturesState() });
  },

  clearKeys: async (venue) => {
    const keys = await commands.clearFuturesKeys(venue);
    set({ keys, testResult: null, state: await commands.getFuturesState() });
  },

  refresh: async () => {
    // Never rejects: a failed poll comes back inside the state, next to the last good snapshot.
    set({ state: await commands.refreshFutures() });
  },

  testConnection: async () => {
    try {
      set({ testResult: await commands.testFuturesConnection() });
    } catch (e) {
      set({ testResult: String(e) });
    }
  },

  // The backend folds a fresh snapshot into `futures` state and emits it as part of handling
  // these, so no `set()` here — `useFutures`'s event listener is what applies the result. These
  // wrappers exist to let a caller `await` the specific request it made and see *its* failure.
  placeOrder: (symbol, side, orderType, quantity, price, reduceOnly) =>
    commands.placeFuturesOrder(symbol, side, orderType, quantity, price, reduceOnly),
  closePosition: (symbol, side, quantity) => commands.closeFuturesPosition(symbol, side, quantity),
  cancelOrder: (symbol, orderId) => commands.cancelFuturesOrder(symbol, orderId),
  setLeverage: (symbol, leverage) => commands.setFuturesLeverage(symbol, leverage),
  setMarginType: (symbol, isolated) => commands.setFuturesMarginType(symbol, isolated),

  loadOrderHistory: async (symbol) => {
    if (get().orderHistoryLoading) return;
    set({ orderHistorySymbol: symbol, orderHistoryLoading: true, orderHistoryError: null });
    try {
      const orderHistory = await commands.getFuturesOrderHistory(symbol);
      set({ orderHistory, orderHistoryLoading: false });
    } catch (e) {
      set({ orderHistoryError: String(e), orderHistoryLoading: false });
    }
  },
}));
