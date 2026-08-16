import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  message: string | null;
}

/**
 * Without a boundary, any throw during render tears down the whole tree: the window keeps its
 * panel geometry while React remounts from scratch with `expanded: false`, so the widget looks
 * like it spat the user back out to the pill for no reason. Surfacing the message instead makes
 * that failure mode diagnosable rather than mysterious.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { message: null };

  static getDerivedStateFromError(error: unknown): State {
    return { message: error instanceof Error ? error.message : String(error) };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    console.error("crypto-widget crashed", error, info.componentStack);
  }

  render() {
    if (this.state.message === null) {
      return this.props.children;
    }

    return (
      <div className="crash">
        <span className="crash__title">Widget crashed</span>
        <span className="crash__message">{this.state.message}</span>
        <button className="btn" onClick={() => this.setState({ message: null })}>
          Retry
        </button>
      </div>
    );
  }
}
