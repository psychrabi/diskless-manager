import React, { useState, useEffect } from 'react';
import { Card, Button } from '../components/ui';
import { RefreshCw } from 'lucide-react';
import { apiRequest } from '../utils/apiRequest';

export const RAMUsage = () => {
    const [ramUsage, setRamUsage] = useState(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(null);

    const fetchRamUsage = async () => {
        try {
            setLoading(true);
            const response = await apiRequest('/system/ram', 'GET');
            console.log('RAM usage response:', response);
            if (response) {
                setRamUsage(response);
                setError(null);
            } else {
                setError('Failed to fetch RAM usage');
            }
        } catch (err) {
            console.error('Error fetching RAM usage:', err);
            setError('Failed to fetch RAM usage');
        } finally {
            setLoading(false);
        }
    };

    const clearRamCache = async () => {
        try {
            const response = await apiRequest('/system/ram/clear', 'POST');
            if (response) {
                console.log('RAM clear response:', response);

                // Refresh RAM usage after clearing cache
                fetchRamUsage();
            } else {
                setError('Failed to clear RAM cache');
            }
        } catch (err) {
            setError('Failed to clear RAM cache');
        }
    };

    useEffect(() => {
        fetchRamUsage();
        // Refresh every 30 seconds
        const interval = setInterval(fetchRamUsage, 30000);
        return () => clearInterval(interval);
    }, []);

    if (loading) {
        return (
            <Card title="RAM Usage" icon={RefreshCw}>
                <div className="text-center py-4 text-gray-500">Loading RAM usage...</div>
            </Card>
        );
    }

    if (error) {
        return (
            <Card title="RAM Usage" icon={RefreshCw}>
                <div className="text-red-500">Error: {error}</div>
            </Card>
        );
    }

    return (
        <Card title="RAM Usage" icon={RefreshCw}>
            <div className="space-y-6">
                <div className="grid grid-cols-2 gap-4">
                    <div className="space-y-2">
                        <h4 className="text-sm font-medium text-gray-500">Memory</h4>
                        <div className="space-y-1">
                            <div>Total: {ramUsage.memory.total}</div>
                            <div>Used: {ramUsage.memory.used}</div>
                            <div>Free: {ramUsage.memory.free}</div>
                            <div>Available: {ramUsage.memory.available}</div>
                        </div>
                    </div>
                    <div className="space-y-2">
                        <h4 className="text-sm font-medium text-gray-500">Swap</h4>
                        <div className="space-y-1">
                            <div>Total: {ramUsage.swap.total}</div>
                            <div>Used: {ramUsage.swap.used}</div>
                            <div>Free: {ramUsage.swap.free}</div>
                        </div>
                    </div>
                </div>
                <Button
                    onClick={clearRamCache}
                    variant="outline"
                    className="w-full"
                >
                    Clear RAM Cache
                </Button>
            </div>
        </Card>
    );
};
