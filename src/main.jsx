import ErrorBoundary from '@/components/ErrorBoundary'
import { Loading } from '@/components/ui'
import { AuthProvider } from '@/contexts/AuthContext'
import { ConfirmDialogProvider } from '@/contexts/ConfirmDialogContext.jsx'
import { NotificationProvider } from '@/contexts/NotificationContext'
import { ThemeProvider } from '@/contexts/ThemeContext'
import '@/index.css'
import { router } from '@/router/router'
import { StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <ErrorBoundary>
      <Suspense fallback={<Loading />}>
        <NotificationProvider>
          <AuthProvider>
            <ConfirmDialogProvider>
              <ThemeProvider>
                <RouterProvider router={router} />
              </ThemeProvider>
            </ConfirmDialogProvider>
          </AuthProvider>
        </NotificationProvider>
      </Suspense>
    </ErrorBoundary >
  </StrictMode >
)