import type { Timeframe } from "../../types/market";

const TIMEFRAMES: Timeframe[] = ["1h", "4h", "1d", "1w"];

interface Props {
  value: Timeframe;
  onChange: (tf: Timeframe) => void;
}

export function TimeframeBar({ value, onChange }: Props) {
  return (
    <div className="timeframe-bar">
      {TIMEFRAMES.map((tf) => (
        <button
          key={tf}
          className={`timeframe-bar__btn ${tf === value ? "timeframe-bar__btn--active" : ""}`}
          onClick={() => onChange(tf)}
        >
          {tf.toUpperCase()}
        </button>
      ))}
    </div>
  );
}
