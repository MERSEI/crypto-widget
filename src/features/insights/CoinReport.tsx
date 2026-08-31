import { formatAge } from "../../core/format/age";
import { formatPercent } from "../../core/format/percent";
import { formatVolume } from "../../core/format/volume";
import { useInsightsStore } from "../../core/store/insights";
import type { CoinInsight, Fundamentals, Verdict } from "../../types/insights";

const VERDICT_LABEL: Record<Verdict, string> = {
  bullish: "BULLISH",
  neutral: "NEUTRAL",
  bearish: "BEARISH",
};

/** The measured half of the card. Rendered as its own block, labelled with its source, because
 *  the whole design rests on a reader being able to tell CoinGecko's figures from the model's
 *  prose without reading carefully. */
function FundamentalsGrid({ fundamentals }: { fundamentals: Fundamentals }) {
  const supplyPct =
    fundamentals.circulatingSupply && fundamentals.maxSupply
      ? (fundamentals.circulatingSupply / fundamentals.maxSupply) * 100
      : null;

  const rows: Array<[string, string]> = [];
  if (fundamentals.marketCapRank) rows.push(["Rank", `#${fundamentals.marketCapRank}`]);
  if (fundamentals.marketCapUsd) rows.push(["Mkt cap", `$${formatVolume(fundamentals.marketCapUsd)}`]);
  if (fundamentals.volume24hUsd) rows.push(["Vol 24h", `$${formatVolume(fundamentals.volume24hUsd)}`]);
  if (fundamentals.athChangePct !== null)
    rows.push(["From ATH", formatPercent(fundamentals.athChangePct)]);
  if (supplyPct !== null) rows.push(["Supply out", `${supplyPct.toFixed(0)}%`]);
  if (fundamentals.sentimentUpPct !== null)
    rows.push(["CG sentiment", `${fundamentals.sentimentUpPct.toFixed(0)}% up`]);

  if (rows.length === 0) return null;

  return (
    <div className="insights-card__block">
      <div className="insights-card__block-title">CoinGecko — measured</div>
      <div className="insights-facts">
        {rows.map(([label, value]) => (
          <div className="insights-facts__cell" key={label}>
            <span className="insights-facts__label">{label}</span>
            <span className="insights-facts__value mono-nums">{value}</span>
          </div>
        ))}
      </div>
      {fundamentals.categories.length > 0 && (
        <div className="insights-card__tags">
          {fundamentals.categories.map((category) => (
            <span className="insights-tag" key={category}>
              {category}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export function CoinReport({ insight }: { insight: CoinInsight }) {
  const openUrl = useInsightsStore((s) => s.openUrl);
  const { analysis } = insight;

  return (
    <div className="insights-card">
      <div className="insights-card__verdict">
        <span className={`insights-verdict insights-verdict--${analysis.verdict}`}>
          {VERDICT_LABEL[analysis.verdict]}
        </span>
        <div className="insights-score">
          <div className="insights-score__bar">
            <div
              className={`insights-score__fill insights-score__fill--${analysis.verdict}`}
              style={{ width: `${Math.max(2, Math.min(100, analysis.score))}%` }}
            />
          </div>
          <span className="insights-score__value mono-nums">{analysis.score}</span>
        </div>
      </div>

      {analysis.summary && <p className="insights-card__summary">{analysis.summary}</p>}

      {analysis.catalysts.length > 0 && (
        <div className="insights-card__block">
          <div className="insights-card__block-title">Catalysts</div>
          <ul className="insights-list insights-list--up">
            {analysis.catalysts.map((item, index) => (
              <li key={index}>{item}</li>
            ))}
          </ul>
        </div>
      )}

      {analysis.risks.length > 0 && (
        <div className="insights-card__block">
          <div className="insights-card__block-title">Risks</div>
          <ul className="insights-list insights-list--down">
            {analysis.risks.map((item, index) => (
              <li key={index}>{item}</li>
            ))}
          </ul>
        </div>
      )}

      {analysis.news.length > 0 && (
        <div className="insights-card__block">
          <div className="insights-card__block-title">News</div>
          {analysis.news.map((item) => (
            <button
              className="insights-news"
              key={item.url}
              title={item.url}
              onClick={() => void openUrl(item.url)}
            >
              <span className={`insights-news__dot insights-news__dot--${item.sentiment}`} />
              <span className="insights-news__text">
                <span className="insights-news__title">{item.title}</span>
                {item.impact && <span className="insights-news__impact">{item.impact}</span>}
                <span className="insights-news__meta">
                  {[item.source, item.published].filter(Boolean).join(" · ")}
                </span>
              </span>
            </button>
          ))}
        </div>
      )}

      {insight.fundamentals && <FundamentalsGrid fundamentals={insight.fundamentals} />}

      {insight.sources.length > 0 && (
        <div className="insights-card__block">
          <div className="insights-card__block-title">Sources read ({insight.sources.length})</div>
          <div className="insights-card__tags">
            {insight.sources.map((source) => (
              <button
                className="insights-tag insights-tag--link"
                key={source.url}
                title={source.url}
                onClick={() => void openUrl(source.url)}
              >
                {hostOf(source.url)}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="insights-card__footer">
        <span>{insight.model}</span>
        <span className="mono-nums">
          {insight.usage.webSearches} searches ·{" "}
          {(insight.usage.inputTokens + insight.usage.outputTokens).toLocaleString()} tok
        </span>
        <span>
          {formatAge(insight.generatedAt)}
          {insight.cached && " · cached"}
        </span>
      </div>
    </div>
  );
}

/** `https://www.coindesk.com/markets/…` → `coindesk.com`. A citation strip is a list of *who*
 *  said it; the full URL is on the title attribute and one click away. */
export function hostOf(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}
