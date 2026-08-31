import { useUiStore, type PanelTab } from "../../core/store/ui";

const TABS: Array<{ id: PanelTab; label: string; title: string }> = [
  { id: "watch", label: "WATCH", title: "Watchlist" },
  { id: "ai", label: "AI", title: "AI research" },
  { id: "futures", label: "FUT", title: "Futures positions" },
  { id: "referral", label: "REF", title: "Referral links" },
];

/**
 * Tab strip under the panel header.
 *
 * Its own row rather than part of `panel__header`: the header carries the drag handle
 * (`onPointerDown` → `start_dragging`), which hands the pointer to the OS move loop — a click
 * meant for a tab would have moved the window instead.
 */
export function PanelTabs() {
  const activeTab = useUiStore((s) => s.activeTab);
  const setActiveTab = useUiStore((s) => s.setActiveTab);

  return (
    <div className="panel__tabs">
      {TABS.map((tab) => (
        <button
          key={tab.id}
          className={`panel-tab ${activeTab === tab.id ? "panel-tab--active" : ""}`}
          title={tab.title}
          onClick={() => setActiveTab(tab.id)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}
