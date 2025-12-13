import { Error, Loading, Notification } from '@/components/ui';
import ErrorBoundary from '@/components/ErrorBoundary';
import { useNotification } from '@/contexts/notification';
import { useAppStore } from '@/store/useAppStore';
import { Activity, lazy, useEffect, useState, useRef } from 'react';
import { Outlet, useNavigation, useLocation, useNavigate } from 'react-router-dom';
import { useToastStore } from '@/store/useToastStore';
import Toast from '../ui/Toast';

const Sidebar = lazy(() => import("@/components/layouts/Sidebar"));
const Header = lazy(() => import("@/components/layouts/Header"));

const AdminLayout = () => {
	const { error, fetchData, loading } = useAppStore()
	const [activeTab, setActiveTab] = useState('dashboard');
	const { notification } = useNotification()
	const navigation = useNavigation();
	const isNavigating = Boolean(navigation.location);
	const [isSidebarOpen, setIsSidebarOpen] = useState(false);
	const [isSidebarCollapsed, setIsSidebarCollapsed] = useState(false);
	const { toasts } = useToastStore();


	const toggleSidebarCollapse = () => {
		setIsSidebarCollapsed(prevState => !prevState);
	};

	const location = useLocation();
	const navigate = useNavigate();

	useEffect(() => {
		fetchData()
	}, [fetchData]);

	// Restore path on mount if we are at root and have a saved path
	useEffect(() => {
		const lastPath = localStorage.getItem('last_path');
		// Determine if we should navigate:
		// Logic: If on root '/' AND we have a saved path that is not '/', restore it.
		// Note with HashRouter: window.location.hash might be '#/' initially.
		if (lastPath && lastPath !== '/' && location.pathname === '/') {
			navigate(lastPath);
		}
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, []); // Run once on mount

	// Save current path
	const isFirstRun = useRef(true);
	useEffect(() => {
		if (isFirstRun.current) {
			isFirstRun.current = false;
			return;
		}
		if (location.pathname && location.pathname !== '/login') {
			localStorage.setItem('last_path', location.pathname);
		}
	}, [location]);



	return (
		<div className="flex h-screen bg-base-200 text-base-content">
			{/* Skip Navigation Link */}
			<a
				href="#main-content"
				className="sr-only focus:not-sr-only focus:absolute focus:top-4 focus:left-4 focus:z-50 focus:px-4 focus:py-2 focus:bg-primary focus:text-primary-content focus:rounded"
			>
				Skip to main content
			</a>

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
					aria-label="Close sidebar"
					role="button"
					tabIndex={0}
					onKeyDown={(e) => e.key === 'Enter' && setIsSidebarOpen(false)}
				/>
			</Activity>

			<div className="flex-1 flex flex-col overflow-hidden"
				style={{
					marginLeft: isSidebarCollapsed ? 'var(--sidebar-width-collapsed)' : 'var(--sidebar-width-open)',
					transition: 'margin-left 0.3s ease-in-out',
				}}>
				<Header onToggleSidebar={() => setIsSidebarOpen((v) => !v)} />
				<main id="main-content" className="flex-1 overflow-y-auto bg-base-200" tabIndex={-1}>
					<div className="p-6 relative">
						{/* Global Loading Overlay */}
						{loading && (
							<div className="absolute inset-0 z-50 flex items-center justify-center bg-base-200/50 backdrop-blur-sm rounded-lg" role="status" aria-live="polite">
								<Loading className="w-10 h-10 text-primary" />
								<span className="sr-only">Loading...</span>
							</div>
						)}
						{error && <Error error={error} />}
						{notification && <Notification />}
						{isNavigating ? <Loading /> : (
							<ErrorBoundary>
								<Outlet />
							</ErrorBoundary>
						)}
					</div>
				</main>
			</div>
			<div className="fixed bottom-4 right-4 z-50 space-y-2">
				{toasts.map((toast) => (
					<Toast key={toast.id} toast={toast} />
				))}
			</div>
		</div>
	)
}

export default AdminLayout;