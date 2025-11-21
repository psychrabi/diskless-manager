import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useZfs } from './useZfs';
import * as tauriCore from '@tauri-apps/api/core';

// Mock Tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(),
}));

// Mock notification context
vi.mock('@/contexts/notification', () => ({
    useNotification: () => ({
        showNotification: vi.fn(),
    }),
}));

describe('useZfs', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        // Mock localStorage
        global.localStorage = {
            getItem: vi.fn(() => 'mock-token'),
            setItem: vi.fn(),
            removeItem: vi.fn(),
            clear: vi.fn(),
        };
    });

    it('initializes with empty datasets and not loading', () => {
        const { result } = renderHook(() => useZfs());

        expect(result.current.datasets).toEqual([]);
        expect(result.current.loading).toBe(false);
    });

    it('fetchDatasets sets loading state correctly', async () => {
        tauriCore.invoke.mockImplementation(() => new Promise(resolve => {
            setTimeout(() => resolve(['dataset1', 'dataset2']), 100);
        }));

        const { result } = renderHook(() => useZfs());

        // Start fetching
        result.current.fetchDatasets('tank');

        // Should be loading initially
        await waitFor(() => {
            expect(result.current.loading).toBe(true);
        });

        // Should finish loading and have datasets
        await waitFor(() => {
            expect(result.current.loading).toBe(false);
            expect(result.current.datasets).toEqual(['dataset1', 'dataset2']);
        });
    });

    it('fetchDatasets clears datasets when pool is null', async () => {
        const { result } = renderHook(() => useZfs());

        // Set some initial datasets
        tauriCore.invoke.mockResolvedValue(['dataset1']);

        await result.current.fetchDatasets('tank');

        await waitFor(() => {
            expect(result.current.datasets).toEqual(['dataset1']);
        });

        // Now fetch with null pool
        await result.current.fetchDatasets(null);

        await waitFor(() => {
            expect(result.current.datasets).toEqual([]);
        });

        expect(tauriCore.invoke).toHaveBeenCalledTimes(1); // Only called once for 'tank'
    });

    it('createDataset returns true on success', async () => {
        tauriCore.invoke.mockResolvedValue({ success: true });

        const { result } = renderHook(() => useZfs());

        const success = await result.current.createDataset({
            zpool: 'tank',
            name: 'test-dataset',
            usage_type: 'image',
            size: '10G',
        });

        expect(success).toBe(true);
        expect(tauriCore.invoke).toHaveBeenCalledWith('create_zfs_dataset', {
            req: {
                zpool: 'tank',
                name: 'test-dataset',
                usage_type: 'image',
                size: '10G',
            }
        });
    });

    it('createDataset returns false on error', async () => {
        tauriCore.invoke.mockRejectedValue(new Error('Creation failed'));

        const { result } = renderHook(() => useZfs());

        const success = await result.current.createDataset({
            zpool: 'tank',
            name: 'test-dataset',
            usage_type: 'image',
        });

        expect(success).toBe(false);
    });

    it('deleteDataset returns true on success', async () => {
        tauriCore.invoke.mockResolvedValue({ message: 'Deleted successfully' });

        const { result } = renderHook(() => useZfs());

        const success = await result.current.deleteDataset('tank/images/ubuntu');

        expect(success).toBe(true);
        expect(tauriCore.invoke).toHaveBeenCalledWith('delete_zfs_dataset', {
            token: 'mock-token',
            dataset: 'tank/images/ubuntu',
            recursive: true,
        });
    });

    it('deleteDataset returns false on error', async () => {
        tauriCore.invoke.mockRejectedValue({ error: 'Deletion failed' });

        const { result } = renderHook(() => useZfs());

        const success = await result.current.deleteDataset('tank/images/ubuntu');

        expect(success).toBe(false);
    });
});
