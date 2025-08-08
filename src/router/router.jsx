import { lazy } from "react";
import { createBrowserRouter } from "react-router-dom";

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
			},
			{
				path: "/clients",
				Component: ClientManagement,
			},
			{
				path: "/masters",
				Component: ImageManagement,
			},
			{
				path: "/settings",
				Component: SettingManagement,
			},
			{
				path: "/setup",
				Component: Setup,
			},
		],
	},
])