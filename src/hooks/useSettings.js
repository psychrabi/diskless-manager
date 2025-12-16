import { useToastStore } from '@/store/useToastStore';
import { invoke } from '@tauri-apps/api/core';
import { useCallback, useState } from 'react';

export const useSettings = () => {
    const [loading, setLoading] = useState(false);
    const { error, success } = useToastStore();

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
            success('DHCP configuration saved successfully', 'success');
            return true;
        } catch (err) {
            error('Failed to configure DHCP server', err.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [success, error]);

    const updateTftp = useCallback(async (tftpConfig) => {
        setLoading(true);
        const token = localStorage.getItem('authToken') || '';
        try {
            const response = await invoke('configure_tftp_server', { token, tftpConfig });
            if (response.message) success(response.message, 'success');
            return true;
        } catch (err) {
            error('Failed to configure TFTP server', err.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [success, error]);

    const updateHttp = useCallback(async (httpConfig) => {
        setLoading(true);
        const token = localStorage.getItem('authToken') || '';
        try {
            const response = await invoke('configure_apache_server', { token, httpConfig });
            if (response.message) success(response.message, 'success');
            return true;
        } catch (err) {
            error('Failed to configure HTTP server', err.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [error, success]);

    const updatePassword = useCallback(async (oldPassword, newPassword) => {
        setLoading(true);
        const token = localStorage.getItem('authToken') || '';
        try {
            const response = await invoke('update_admin_password', { token, oldPassword, newPassword });
            if (response) success(response);
            return true;
        } catch (err) {
            error('Failed to update admin password', err.message || 'An unknown error occurred');
            return false;
        } finally {
            setLoading(false);
        }
    }, [error, success]);

    const getLicenseInfo = useCallback(async () => {
        try {
            return await invoke('get_license_info');
        } catch (err) {
            error('Failed to load license info', err?.message || String(err));
            return null;
        }
    }, [error]);

    const activateLicense = useCallback(async (key) => {
        setLoading(true);
        try {
            const resp = await invoke('activate_license', { key });
            success('License Activated', resp?.message || 'License activated successfully');
            return true;
        } catch (err) {
            error('License Activation Failed', err?.message || String(err));
            return false;
        } finally {
            setLoading(false);
        }
    }, [success, error]);

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
