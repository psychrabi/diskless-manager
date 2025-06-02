import { invoke } from "@tauri-apps/api/core"
import { createBrowserRouter } from "react-router"
import ClientManagement from "../components/ClientManagement"
import ImageManagement from "../components/ImageManagement"
import ServiceManagement from "../components/ServiceManagement"
import SetupPage from "../components/SetupPage"
import { MainLayout } from "../layouts/MainLayout"

export const router = createBrowserRouter([
	{
		path: "/",
		Component: MainLayout,
		children: [
			{
				path: "/",
				Component: ServiceManagement,
				loader: async () => {
					const services = await invoke('get_services', { 'zfsPool': 'diskless' });
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
				path: "/setup",
				Component: SetupPage,
			},
		],
	},
])