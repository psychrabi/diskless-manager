import { invoke } from "@tauri-apps/api/core"
import { lazy } from "react";
import { createBrowserRouter } from "react-router"

const MainLayout = lazy(() => import("@/components/layouts/MainLayout"));
const ClientManagement = lazy(() => import("@/components/ClientManagement"));
const ImageManagement = lazy(() => import("@/components/ImageManagement"));
const ServiceManagement = lazy(() => import("@/components/ServiceManagement"));
const Setup = lazy(() => import("@/components/Setup"));
const SettingManagement = lazy(() => import("@/components/SettingsManagement"));

export const router = createBrowserRouter([
	{
		path: "/",
		Component: MainLayout,
		children: [
			{
				path: "/",
				Component: ServiceManagement,
				loader: async () => {
					const services = await invoke('check_package_status', { 'zfsPool': 'diskless' });
					console.log(services)
					return { services };
				},
			},
			{
				path: "/clients",
				Component: ClientManagement,
				loader: async () => {
					const clients = await invoke('get_clients', { 'zfsPool': 'diskless' });
					return { clients };
				},
			},
			{
				path: "/masters",
				Component: ImageManagement,
				loader: async () => {
					const masters = await invoke('get_masters', { 'zfsPool': 'diskless' });
					return { masters };
				},
			},
			{
				path: "/settings",
				Component: SettingManagement,
				// loader: async () => {
				// 	const masters = await invoke('get_masters', { 'zfsPool': 'diskless' });
				// 	return { masters };
				// },
			},
			{
				path: "/setup",
				Component: Setup,
			},
		],
	},
])