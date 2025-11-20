import { invoke } from '@tauri-apps/api/core';
import { useState, useCallback } from 'react';
import { useNotification } from '@/contexts/notification';

export const useLogs = () => {
    const [logs, setLogs] = useState(null);
    const [loading, setLoading] = useState(false);
    const { showNotification } = useNotification();

    const fetchLogs = useCallback(async (unit, lines = 50) => {
        if (!unit) return;
        setLoading(true);
        try {
            const out = await invoke('get_service_logs', { unit, lines });
            setLogs(out);
        } catch (error) {
            console.error(error);
            showNotification('error', 'Failed to fetch logs', error || 'Unknown error');
        } finally {
            setLoading(false);
        }
    }, [showNotification]);

    return {
        logs,
        loading,
        fetchLogs
    };
};
