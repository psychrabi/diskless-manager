import { invoke } from '@tauri-apps/api/core';
import { useState, useCallback } from 'react';
import { useNotification } from '@/contexts/notification';

export const useSettings = () => {
    const [loading, setLoading] = useState(false);
    const { showNotification } = useNotification();

    const readConfig = useCallback(async () => {
        try {
            return await invoke('read_config');
        } catch (error) {
            console.error('Failed to load config:', error);
            return null;
        }
    }, []);

    const updateDhcp = useCallback(async (config) => {
        setLoading(true);
        const token = localStorage.getItem('authToken') || '';
        try {
            await invoke('configure_dhcp_server', { token, config });
            showNotification('DHCP configuration saved successfully', 'success');
            return true;
        } catch (error) {
            showNotification('error', 'Failed to configure DHCP server', error.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [showNotification]);

    const updateTftp = useCallback(async (tftpConfig) => {
        setLoading(true);
        const token = localStorage.getItem('authToken') || '';
        try {
            const response = await invoke('configure_tftp_server', { token, tftpConfig });
            if (response.message) showNotification(response.message, 'success');
            return true;
        } catch (error) {
            showNotification('error', 'Failed to configure TFTP server', error.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [showNotification]);

    const updateHttp = useCallback(async (httpConfig) => {
        setLoading(true);
        const token = localStorage.getItem('authToken') || '';
        try {
            const response = await invoke('configure_apache_server', { token, httpConfig });
            if (response.message) showNotification(response.message, 'success');
            return true;
        } catch (error) {
            showNotification('error', 'Failed to configure HTTP server', error.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [showNotification]);

    const updatePassword = useCallback(async (oldPassword, newPassword) => {
        setLoading(true);
        const token = localStorage.getItem('authToken') || '';
        try {
            const response = await invoke('update_admin_password', { token, oldPassword, newPassword });
            if (response) showNotification(response, 'success');
            return true;
        } catch (error) {
            showNotification('error', 'Failed to update admin password', error.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [showNotification]);

    const getLicenseInfo = useCallback(async () => {
        try {
            return await invoke('get_license_info');
        } catch (e) {
            showNotification('error', 'Failed to load license info', e?.message || String(e));
            return null;
        }
    }, [showNotification]);

    const activateLicense = useCallback(async (key) => {
        setLoading(true);
        try {
            const resp = await invoke('activate_license', { key });
            showNotification('success', 'License Activated', resp?.message || 'License activated successfully');
            return true;
        } catch (e) {
            showNotification('error', 'License Activation Failed', e?.message || String(e));
            return false;
        } finally {
            setLoading(false);
        }
    }, [showNotification]);

    return {
        loading,
        readConfig,
        updateDhcp,
        updateTftp,
        updateHttp,
        updatePassword,
        getLicenseInfo,
        activateLicense
    };
};
