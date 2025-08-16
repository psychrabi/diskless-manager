import ErrorBoundary from '@/components/ErrorBoundary'
import { Loading } from '@/components/ui'
import { NotificationProvider } from '@/contexts/NotificationContext.jsx'
import { AuthProvider } from '@/contexts/AuthContext.jsx'
import '@/index.css'
import { router } from '@/router/router'
import { StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router-dom'

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <ErrorBoundary>
      <Suspense fallback={<Loading />}>
        <AuthProvider>
          <NotificationProvider>
            <RouterProvider router={router} />
          </NotificationProvider>
        </AuthProvider>
      </Suspense>
    </ErrorBoundary>
  </StrictMode>
)