import React, { StrictMode, Suspense } from 'react'
import ReactDOM from 'react-dom/client'
import './index.css'
import App from './App.jsx'
import { NotificationProvider } from './contexts/NotificationContext.jsx'
import { BrowserRouter } from 'react-router-dom'
import ErrorBoundary from './components/ErrorBoundary'

// Loading fallback component
const LoadingFallback = () => (
  <div className="min-h-screen bg-gray-100 dark:bg-gray-900 flex items-center justify-center">
    <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-b-4 border-blue-500"></div>
  </div>
)

ReactDOM.createRoot(document.getElementById('root')).render(
  <StrictMode>
    <ErrorBoundary>
      <Suspense fallback={<LoadingFallback />}>
        <NotificationProvider>
          <BrowserRouter>
            <App />
          </BrowserRouter>

        </NotificationProvider>
      </Suspense>
    </ErrorBoundary>
  </StrictMode>
)
