import { Outlet, useNavigation } from 'react-router'
import {Sidebar} from '@/components/ui/Sidebar'
import React, { useEffect } from 'react'
import { Loading, Notification } from '@/components/ui'
import { useAppStore } from '@/store/useAppStore'
import { useNotification } from '@/contexts/NotificationContext'

export const MainLayout = () => {
	const { error } = useAppStore()
	const { notification } = useNotification()

	const navigation = useNavigation();
  const isNavigating = Boolean(navigation.location);


	return (
		<div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 p-2 md:p-4 font-sans">
			<div className="flex">
				<Sidebar />
				<div className="flex-grow ml-56">
					{error && <Error error={error} />}
					{notification && <Notification />}
					{isNavigating ? <Loading /> : <Outlet />}
				</div>				
			</div>
		</div>
	)
}