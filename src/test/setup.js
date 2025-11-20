import '@testing-library/jest-dom';
import { afterEach, vi } from 'vitest';
import { cleanup } from '@testing-library/react';

// Mock Tauri API to prevent errors in tests
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

// Cleanup after each test
afterEach(() => {
    cleanup();
});

// Setup happy-dom/jsdom polyfills if needed
if (typeof global.TextEncoder === 'undefined') {
    const { TextEncoder, TextDecoder } = require('util');
    global.TextEncoder = TextEncoder;
    global.TextDecoder = TextDecoder;
}
