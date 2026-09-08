import { Component } from "react";
import ErrorFallback from "./ErrorFallback";
import { clearSavedRoute, returnToHome } from "@/lib/error-recovery";

const initialState = { hasError: false, error: null, componentStack: "" };

export default class ErrorBoundary extends Component {
  state = initialState;

  static getDerivedStateFromError(error) {
    return { hasError: true, error };
  }

  componentDidCatch(error, info) {
    this.setState({ componentStack: info.componentStack || "" });
    console.error("[ErrorBoundary]", error, info.componentStack);
  }

  componentDidUpdate(prevProps, prevState) {
    const previous = prevProps.resetKeys || [];
    const current = this.props.resetKeys || [];
    if (this.state.hasError && prevState.hasError && (
      previous.length !== current.length || current.some((value, index) => !Object.is(value, previous[index]))
    )) {
      this.reset();
    }
  }

  reset = () => this.setState(initialState);

  goHome = () => {
    clearSavedRoute();
    if (this.props.onHome) {
      this.props.onHome();
      this.reset();
    } else {
      returnToHome();
    }
  };

  render() {
    if (!this.state.hasError) return this.props.children;
    return (
      <ErrorFallback
        error={this.state.error}
        componentStack={this.state.componentStack}
        fullPage={this.props.fullPage}
        showDetails={this.props.showDetails}
        onRetry={this.reset}
        onHome={this.goHome}
      />
    );
  }
}
