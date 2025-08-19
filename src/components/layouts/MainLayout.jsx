import { Loading, Notification } from '@/components/ui'
import { useNotification } from '@/contexts/NotificationContext'
import { useAppStore } from '@/store/useAppStore'
import { lazy, useEffect, useState } from 'react'
import { Outlet, useNavigation } from 'react-router'

const Sidebar = lazy(() => import("@/components/layouts/Sidebar"));
const Header = lazy(() => import("@/components/layouts/Header"));

const MainLayout = () => {
	const { error, fetchData } = useAppStore()
	const [activeTab, setActiveTab] = useState('dashboard');
	const { notification } = useNotification()
	const navigation = useNavigation();
	const isNavigating = Boolean(navigation.location);
	const [isSidebarOpen, setIsSidebarOpen] = useState(false);

	useEffect(() => {
		fetchData()
	}, [fetchData]);

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
			/>

			{/* Backdrop on small screens */}
			{isSidebarOpen && (
				<div
					className="fixed inset-0 z-30 bg-black/50 lg:hidden"
					onClick={() => setIsSidebarOpen(false)}
				/>
			)}

			<div className="flex-1 flex flex-col overflow-hidden">
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