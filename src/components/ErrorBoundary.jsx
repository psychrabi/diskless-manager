import React from 'react';
import { Button } from './ui';
import { AlertTriangle, RefreshCw, Home } from 'lucide-react';

class ErrorBoundary extends React.Component {
  constructor(props) {
    super(props);
    this.state = { hasError: false, error: null, errorInfo: null };
  }

  static getDerivedStateFromError(error) {
    return { hasError: true };
  }

  componentDidCatch(error, errorInfo) {
    this.setState({ error, errorInfo });
    console.error("Uncaught error:", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen flex items-center justify-center bg-base-200 p-4">
          <div className="card w-full max-w-lg bg-base-100 shadow-xl">
            <div className="card-body items-center text-center">
              <AlertTriangle className="h-12 w-12 text-error mb-4" />
              <h2 className="card-title text-2xl mb-2">Something went wrong</h2>
              <p className="text-base-content/70 mb-6">
                An unexpected error occurred. Please try reloading the page.
              </p>
              <div className="flex gap-4">
                <Button
                  variant="primary"
                  onClick={() => window.location.reload()}
                  className="gap-2"
                >
                  <RefreshCw className="h-4 w-4" />
                  Reload Page
                </Button>
                <Button
                  variant="outline"
                  onClick={() => window.location.href = '/'}
                  className="gap-2"
                >
                  <Home className="h-4 w-4" />
                  Go Home
                </Button>
              </div>
              {process.env.NODE_ENV === 'development' && (
                <div className="mt-6 text-left w-full collapse collapse-arrow border border-base-300 bg-base-200 rounded-box">
                  <input type="checkbox" />
                  <div className="collapse-title font-medium">
                    Error Details
                  </div>
                  <div className="collapse-content">
                    <pre className="text-xs overflow-auto max-h-40 p-2">
                      {this.state.error && this.state.error.toString()}
                      <br />
                      {this.state.errorInfo && this.state.errorInfo.componentStack}
                    </pre>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default ErrorBoundary; 