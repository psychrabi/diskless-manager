import { useNotification } from "@/contexts/notification";
import { useAppStore } from "@/store/useAppStore";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { Outlet, useNavigate } from "react-router-dom";
import { Loading } from "../ui";

const PublicLayout = () => {

    const { setServices } = useAppStore();
    const [preflightLoading, setPreflightLoading] = useState(true);
    const navigate = useNavigate();
    const { showNotification } = useNotification();
    // Preflight check before showing login
    useEffect(() => {
        let cancelled = false;
        (async () => {
            try {
                const res = await invoke('check_package_status');
                const list = Array.isArray(res) ? res : (res ? Object.values(res) : []);
                if (!cancelled) {
                    setServices(list);
                    const allServicesInstalled = list.every(svc => svc?.installed);
                    const poolExists = await invoke('zfs_pool_exists');

                    // Only redirect to setup if services are not installed
                    if (!allServicesInstalled || !poolExists) {
                        navigate('/setup');
                    }
                }
            } catch (e) {
                showNotification('error', 'Preflight Check Failed', e.message || 'An unknown error occurred during preflight checks.');
                console.warn('Preflight check failed:', e);
                // Proceed to login UI even if preflight fails
            } finally {
                if (!cancelled) setPreflightLoading(false);
            }

            try {
                const license = await invoke("get_license")
                if (license) console.log(license);
            } catch (error) {
                console.log(error)
            }
        })();
        return () => {
            cancelled = true;
        };
    }, [navigate, setServices, showNotification]);


    if (preflightLoading) {
        return <Loading message="Performing preflight checks..." />;
    }


    return (<div className="min-h-screen flex items-center justify-center bg-base-200 text-base-content p-4">

        <Outlet />
    </div>);
}

export default PublicLayout;
