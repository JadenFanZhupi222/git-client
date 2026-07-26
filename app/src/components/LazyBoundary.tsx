import {
  Component,
  Suspense,
  type ErrorInfo,
  type ReactNode,
} from "react";

interface LazyBoundaryProps {
  children: ReactNode;
  loading: ReactNode;
  message: string;
  retryLabel: string;
  onRetry?: () => void;
}

interface LazyBoundaryState {
  failed: boolean;
}

export class LazyBoundary extends Component<
  LazyBoundaryProps,
  LazyBoundaryState
> {
  state: LazyBoundaryState = { failed: false };

  static getDerivedStateFromError(): LazyBoundaryState {
    return { failed: true };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("Lazy UI chunk failed to render", error, info);
  }

  private retry = () => {
    if (this.props.onRetry) {
      this.props.onRetry();
      return;
    }
    window.location.reload();
  };

  render() {
    if (this.state.failed) {
      return (
        <div
          role="alert"
          className="grid h-full min-h-24 place-items-center gap-3 p-6 text-center text-sm text-fg-muted"
        >
          <p>{this.props.message}</p>
          <button
            type="button"
            onClick={this.retry}
            className="rounded-md border border-line-strong bg-elevated px-3 py-1.5 text-xs text-fg transition-colors hover:bg-overlay"
          >
            {this.props.retryLabel}
          </button>
        </div>
      );
    }

    return (
      <Suspense fallback={this.props.loading}>{this.props.children}</Suspense>
    );
  }
}
