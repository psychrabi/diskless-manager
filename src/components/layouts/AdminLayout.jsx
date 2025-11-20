import { Error, Loading, Notification } from '@/components/ui';
import ErrorBoundary from '@/components/ErrorBoundary';
import { useNotification } from '@/contexts/notification';
import { useAppStore } from '@/store/useAppStore';
import { Activity, lazy, useEffect, useState } from 'react';
import { Outlet, useNavigation } from 'react-router-dom';

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


	const toggleSidebarCollapse = () => {
		setIsSidebarCollapsed(prevState => !prevState);
	};

	useEffect(() => {
		fetchData()
	}, [fetchData]);



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
		</div>
	)
}

export default AdminLayout;