import { StrictMode, Suspense } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from 'react-router'
import ErrorBoundary from '@/components/ErrorBoundary'
import { Loading } from '@/components/ui'
import { NotificationProvider } from '@/contexts/NotificationContext.jsx'
import '@/index.css'
import { router } from '@/router/router'

createRoot(document.getElementById('root')).render(
  <StrictMode>
    <ErrorBoundary>
      <Suspense fallback={<Loading />}>
        <NotificationProvider>
          <RouterProvider router={router} />
        </NotificationProvider>
      </Suspense>
    </ErrorBoundary>
  </StrictMode>
)