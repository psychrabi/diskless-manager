import { Error, Loading, Notification } from '@/components/ui';
import { useNotification } from '@/contexts/notification';
import { useAppStore } from '@/store/useAppStore';
import { invoke } from '@tauri-apps/api/core';
import { Activity, lazy, useEffect, useState } from 'react';
import { Outlet, useNavigate, useNavigation } from 'react-router-dom';

const Sidebar = lazy(() => import("@/components/layouts/Sidebar"));
const Header = lazy(() => import("@/components/layouts/Header"));

const MainLayout = () => {
	const { error, fetchData } = useAppStore()
	const [activeTab, setActiveTab] = useState('dashboard');
	const { notification } = useNotification()
	const navigation = useNavigation();
	const isNavigating = Boolean(navigation.location);
	const [isSidebarOpen, setIsSidebarOpen] = useState(false);
	const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(false);
	const { setServices } = useAppStore();
	const [preflightLoading, setPreflightLoading] = useState(true);
	const navigate = useNavigate();
	const { showNotification } = useNotification();

	const toggleSidebarCollapse = () => {
		setIsSidebarCollapsed(prevState => !prevState);
	};

	useEffect(() => {
		fetchData()
	}, [fetchData]);


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
					const poolExists = await invoke('zfs_pool_exists', { poolName: null });

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


	return (
		<div className="flex h-screen bg-base-200 text-base-content">
			{/* Sidebar */}
			<Sidebar
				activeTab={activeTab}
				onTabChange={(tab) => {
					setActiveTab(tab);
					setIsSidebarOpen(false);
				}}
				isOpen={isSidebarOpen}
				onClose={() => setIsSidebarOpen(false)}
				isCollapsed={isSidebarCollapsed}
				onToggleCollapse={toggleSidebarCollapse}
			/>

			{/* Backdrop on small screens */}
			<Activity mode={isSidebarOpen ? 'visible' : 'hidden'}>
				<div
					className="fixed inset-0 z-30 bg-black/50 lg:hidden"
					onClick={() => setIsSidebarOpen(false)}
				/>
			</Activity>

			<div className="flex-1 flex flex-col overflow-hidden"
				style={{
					marginLeft: isSidebarCollapsed ? 'var(--sidebar-width-collapsed)' : 'var(--sidebar-width-open)',
					transition: 'margin-left 0.3s ease-in-out',
				}}>
				<Header onToggleSidebar={() => setIsSidebarOpen((v) => !v)} />
				<main className="flex-1 overflow-y-auto bg-base-200">
					<div className="p-6">
						{error && <Error error={error} />}
						{notification && <Notification />}
						{isNavigating ? <Loading /> : <Outlet />}
					</div>
				</main>
			</div>
		</div>
	)
}

export default MainLayout;