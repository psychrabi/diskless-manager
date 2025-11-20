import { invoke } from '@tauri-apps/api/core';
import { useState, useCallback } from 'react';
import { useNotification } from '@/contexts/notification';

export const useZfs = () => {
    const [datasets, setDatasets] = useState([]);
    const [loading, setLoading] = useState(false);
    const { showNotification } = useNotification();

    const fetchDatasets = useCallback(async (pool) => {
        if (!pool) {
            setDatasets([]);
            return;
        }
        setLoading(true);
        try {
            const res = await invoke('list_datasets', { zpool: pool });
            setDatasets(res || []);
        } catch (e) {
            showNotification('error', 'Failed to list datasets', e.message || 'An unknown error occurred');
            console.error(String(e));
        } finally {
            setLoading(false);
        }
    }, [showNotification]);

    const createDataset = useCallback(async (data) => {
        const token = localStorage.getItem('authToken') || '';
        try {
            await invoke('create_zfs_dataset', {
                token,
                zpool: data.zpool,
                name: data.name,
                usageType: data.usage_type,
                size: data.size ?? ''
            });
            showNotification('success', 'Dataset Created', `Dataset ${data.name} created successfully.`);
            return true;
        } catch (e) {
            showNotification('error', 'Failed to create dataset', e.message || 'An unknown error occurred');
            return false;
        }
    }, [showNotification]);

    const deleteDataset = useCallback(async (name) => {
        const token = localStorage.getItem('authToken') || '';
        try {
            const response = await invoke('delete_zfs_dataset', { token, dataset: name, recursive: true });
            if (response.message) showNotification(response.message, 'success');
            return true;
        } catch (e) {
            showNotification('error', 'Failed to delete disk', e.error || 'An unknown error occurred');
            return false;
        }
    }, [showNotification]);

    return {
        datasets,
        loading,
        fetchDatasets,
        createDataset,
        deleteDataset
    };
};
