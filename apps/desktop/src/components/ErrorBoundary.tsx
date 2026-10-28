import { Component, type ReactNode } from "react";
import { report } from "../lib/log";

interface State { error: Error | null }

/** Shows the failure instead of a blank window, so bugs are reportable. */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null };
  static getDerivedStateFromError(error: Error): State {
    return { error };
  }
  componentDidCatch(error: Error, info: { componentStack?: string }) {
    report("error", `render error: ${error.message}\n${error.stack ?? ""}\n${info.componentStack ?? ""}`);
  }
  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="crash">
        <h2>Something broke</h2>
        <pre>{String(this.state.error?.stack ?? this.state.error)}</pre>
        <button className="primary" onClick={() => this.setState({ error: null })}>Try again</button>
      </div>
    );
  }
}
