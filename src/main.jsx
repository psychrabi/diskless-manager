import ErrorBoundary from '@/components/ErrorBoundary'
import { Loading } from '@/components/ui'
import { NotificationProvider } from '@/contexts/NotificationContext.jsx'
import { AuthProvider } from '@/contexts/AuthContext.jsx'
import { ThemeProvider } from '@/contexts/ThemeContext.jsx'
import '@/index.css'
import { router } from '@/router/router'
import { StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'
import { ConfirmDialogProvider } from '@/contexts/ConfirmDialogContext.jsx'

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <ErrorBoundary>
      <Suspense fallback={<Loading />}>
        <AuthProvider>
          <ConfirmDialogProvider>
            <NotificationProvider>
              <ThemeProvider>
                <RouterProvider router={router} />
              </ThemeProvider>
            </NotificationProvider>
          </ConfirmDialogProvider>
        </AuthProvider>
      </Suspense>
    </ErrorBoundary >
  </StrictMode >
)