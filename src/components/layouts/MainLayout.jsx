import { Loading, Notification } from '@/components/ui'
import { useNotification } from '@/contexts/NotificationContext'
import { useAppStore } from '@/store/useAppStore'
import { lazy, useState } from 'react'
import { Outlet, useNavigation } from 'react-router'

const Sidebar = lazy(() => import("@/components/layouts/Sidebar"));
const Header = lazy(() => import("@/components/layouts/Header"));

const MainLayout = () => {
	const { error } = useAppStore()
	const [activeTab, setActiveTab] = useState('dashboard');
	const { notification } = useNotification()
	const navigation = useNavigation();
	const isNavigating = Boolean(navigation.location);

	return (
		<div className="flex h-screen bg-gray-900">
			<Sidebar activeTab={activeTab} onTabChange={setActiveTab} />
			<div className="flex-1 flex flex-col overflow-hidden">
				<Header />
				<main className="flex-1 overflow-y-auto">
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