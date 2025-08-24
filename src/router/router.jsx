import { Logs } from "@/components/Logs";
import { lazy } from "react";
import { createBrowserRouter } from "react-router-dom";

const MainLayout = lazy(() => import("@/components/layouts/MainLayout"));
const ClientManagement = lazy(() => import("@/components/ClientManagement"));
const ImageManagement = lazy(() => import("@/components/ImageManagement"));
const ServiceManagement = lazy(() => import("@/components/ServiceManagement"));
const Setup = lazy(() => import("@/components/Setup"));
const SettingManagement = lazy(() => import("@/components/SettingsManagement"));
const Login = lazy(() => import("@/components/Authentication/Login"));
const ProtectedRoute = lazy(() => import("@/components/Authentication/ProtectedRoute"));

export const router = createBrowserRouter([
  {
    path: "/login",
    element: <Login />,
  },
  {
    path: "/setup",
    element: <Setup />,
  },
  {
    path: "/",
    element: <ProtectedRoute><MainLayout /></ProtectedRoute>,
    children: [
      {
        index: true,
        element: <ServiceManagement />,
      },
      {
        path: "/clients",
        element: <ClientManagement />,
      },
      {
        path: "/masters",
        element: <ImageManagement />,
      },
      {
        path: "/settings",
        element: <SettingManagement />,
      },
      {
        path: "/logs",
        element: <Logs />,
      },
    ],
  },
])