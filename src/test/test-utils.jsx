import { render } from '@testing-library/react';
import { NotificationProvider } from '@/contexts/NotificationContext';
import { ConfirmDialogProvider } from '@/contexts/ConfirmDialogContext';
import { ThemeProvider } from '@/contexts/ThemeContext';

/**
 * Custom render function that wraps components with necessary providers
 * @param {React.ReactElement} ui - Component to render
 * @param {Object} options - Render options
 * @returns {Object} Render result
 */
export function renderWithProviders(ui, options = {}) {
    return render(
        <NotificationProvider>
            <ConfirmDialogProvider>
                <ThemeProvider>
                    {ui}
                </ThemeProvider>
            </ConfirmDialogProvider>
        </NotificationProvider>,
        options
    );
}

// Re-export everything from testing library
export * from '@testing-library/react';
export { renderWithProviders as render };
