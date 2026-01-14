import UserManagement from "@/components/UserMangement";
import PublicRoute from "@/components/Authentication/PublicRoute";
import PublicLayout from "@/components/layouts/PublicLayout";
import { lazy } from "react";
import { createHashRouter } from "react-router-dom";

const Adminlayout = lazy(() => import("@/components/layouts/AdminLayout"));
const ClientManagement = lazy(() => import("@/components/ClientManagement"));
const ImageManagement = lazy(() => import("@/components/ImageManagement"));
const Images = lazy(() => import("@/components/Images"));
const Dashboard = lazy(() => import("@/components/Dashboard"));
const Setup = lazy(() => import("@/components/Setup"));
const SettingManagement = lazy(() => import("@/components/SettingsManagement"));
const ServiceManagement = lazy(() => import("@/components/ServiceManagement"));
const DisksManagement = lazy(() => import("@/components/DisksManagement"));
const LicenseManagement = lazy(() => import("@/components/LicenseManagement"));
const ApplicationSettings = lazy(() =>
  import("@/components/ApplicationMangement")
);
const Logs = lazy(() => import("@/components/Logs"));
const Login = lazy(() => import("@/components/Authentication/Login"));
const ProtectedRoute = lazy(() =>
  import("@/components/Authentication/ProtectedRoute")
);

export const router = createHashRouter([
  {
    path: "/",
    element: <PublicLayout />,
    children: [
      {
        path: "/login",
        element: (
          <PublicRoute>
            <Login />
          </PublicRoute>
        ),
      },
      {
        path: "/setup",
        element: <Setup />,
      },
    ],
  },
  {
    path: "/",
    element: (
      <ProtectedRoute>
        <Adminlayout />
      </ProtectedRoute>
    ),
    children: [
      {
        index: true,
        element: <Dashboard />,
      },
      {
        path: "/clients",
        element: <ClientManagement />,
      },
      {
        path: "/disks",
        element: <DisksManagement />,
      },
      // {
      //   path: "/images",
      //   element: <Images />,
      // },
      {
        path: "/images",
        element: <ImageManagement />,
      },
      {
        path: "/services",
        element: <ServiceManagement />,
      },
      {
        path: "/settings",
        element: <SettingManagement />,
      },
      {
        path: "/license",
        element: <LicenseManagement />,
      },
      {
        path: "/application-settings",
        element: <ApplicationSettings />,
      },
            {
        path: "/users",
        element: <UserManagement />,
      },
      {
        path: "/logs",
        element: <Logs />,
      },
    ],
  },
]);
