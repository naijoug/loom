import { Component, lazy, Suspense, type ReactNode } from "react";
import { CardFallback, CardSource } from "./CardSource";

// The OpenUI runtime and component library load only when a card first appears.
const InlineCardView = lazy(() => import("./InlineCardView"));

export interface InlineCardProps {
  source: string;
  streaming: boolean;
  /** Appends the card's plain text to the composer draft; never sends. */
  onInsert?: (text: string) => void;
}

class CardBoundary extends Component<{ source: string; children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: unknown) { console.warn("inline card failed to render", error); }
  componentDidUpdate(previous: Readonly<{ source: string }>) {
    if (this.state.failed && previous.source !== this.props.source) this.setState({ failed: false });
  }
  render() { return this.state.failed ? <CardFallback source={this.props.source} /> : this.props.children; }
}

export function InlineCard(props: InlineCardProps) {
  return <CardBoundary source={props.source}>
    <Suspense fallback={<CardSource source={props.source} />}><InlineCardView {...props} /></Suspense>
  </CardBoundary>;
}
