import { Component, type ReactNode } from "react";
import { useT } from "../../i18n";

function ErrorFallback({ retry }: { retry: () => void }) {
  const t = useT();
  return <section role="alert" className="error-boundary">
    <h2>{t("errorTitle")}</h2><p>{t("errorBody")}</p>
    <button type="button" onClick={retry}>{t("reload")}</button>
  </section>;
}

export class ErrorBoundary extends Component<{ children: ReactNode; resetKey?: string }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidUpdate(previous: Readonly<{ children: ReactNode; resetKey?: string }>) {
    if (this.state.failed && previous.resetKey !== this.props.resetKey) this.setState({ failed: false });
  }
  render() {
    return this.state.failed
      ? <ErrorFallback retry={() => this.setState({ failed: false })} />
      : this.props.children;
  }
}
