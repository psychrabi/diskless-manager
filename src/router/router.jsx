import UserManagement from "@/components/UserManagement";
import PublicRoute from "@/components/Authentication/PublicRoute";
import PublicLayout from "@/components/layouts/PublicLayout";
import RouteErrorBoundary from "@/components/RouteErrorBoundary";
import { lazy } from "react";
import { createHashRouter } from "react-router-dom";

const Adminlayout = lazy(() => import("@/components/layouts/AdminLayout"));
const ClientManagement = lazy(() => import("@/components/ClientManagement"));
const ImageManagement = lazy(() => import("@/components/ImageManagement"));
const Dashboard = lazy(() => import("@/components/Dashboard"));
const Setup = lazy(() => import("@/components/Setup"));
const SettingManagement = lazy(() => import("@/components/SettingsManagement"));
const ServiceManagement = lazy(() => import("@/components/ServiceManagement"));
const DisksManagement = lazy(() => import("@/components/DisksManagement"));
const LicenseManagement = lazy(() => import("@/components/LicenseManagement"));
const ApplicationSettings = lazy(() =>
  import("@/components/ApplicationManagement")
);
const Logs = lazy(() => import("@/components/Logs"));
const SshTester = lazy(() => import("@/components/SshTester/SshTester"));
const Login = lazy(() => import("@/components/Authentication/Login"));
const InitialSetup = lazy(() =>
  import("@/components/Authentication/InitialSetup")
);
const ProtectedRoute = lazy(() =>
  import("@/components/Authentication/ProtectedRoute")
);

export const router = createHashRouter([
  {
    path: "/",
    element: <PublicLayout />,
    errorElement: <RouteErrorBoundary />,
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
        path: "/initial-setup",
        element: (
          <PublicRoute>
            <InitialSetup />
          </PublicRoute>
        ),
      },
      {
        path: "/setup",
        element: <Setup />,
      },
    ].map((route) => ({ ...route, errorElement: <RouteErrorBoundary fullPage={false} /> })),
  },
  {
    path: "/",
    errorElement: <RouteErrorBoundary />,
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
      {
        path: "/ssh-tester",
        element: <SshTester />,
      },
    ].map((route) => ({ ...route, errorElement: <RouteErrorBoundary fullPage={false} /> })),
  },
  {
    path: "*",
    loader: () => { throw new Response(null, { status: 404 }); },
    errorElement: <RouteErrorBoundary />,
  },
]);
