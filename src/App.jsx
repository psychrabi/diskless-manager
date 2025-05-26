import { lazy, useEffect } from "react";
import { Route, Routes } from "react-router-dom";
import { useAppStore } from "./store/useAppStore";


const ClientManagement = lazy(() =>
  import("./components/ClientManagement")
);
const ImageManagement = lazy(() => import("./components/ImageManagement"));
const ServiceManagement = lazy(() =>
  import("./components/ServiceManagement")
);
const Notification = lazy(() => import("./components/ui/Notification.jsx"));
const Sidebar = lazy(() => import("./components/ui/Sidebar.jsx"));
const SetupPage = lazy(() => import("./components/SetupPage.jsx"));

function App() {
  const {

    config,
    error,
    loading,
    fetchData } = useAppStore();



  useEffect(() => {
    fetchData();
  }, [fetchData]);

  return (
    <div className="min-h-screen bg-gray-100 dark:bg-gray-900 text-gray-900 dark:text-gray-100 p-2 md:p-4 font-sans">
      <div className="flex">
        <Sidebar />
        <div className="flex-grow ml-56">
          {/* Global Error Display */}
          {error && (
            <div
              className="bg-red-100 border border-red-400 text-red-700 px-4 py-3 rounded relative mb-6 dark:bg-red-900 dark:border-red-700 dark:text-red-200"
              role="alert"
            >
              <strong className="font-bold mr-2">Error:</strong>
              <span className="block sm:inline">{error}</span>
            </div>
          )}

          <Notification />
          {/* Service Status Cards */}

          <Routes>
            <Route path="/" element={<ServiceManagement />} />
            <Route path="/clients" element={<ClientManagement  />} />
            <Route path="/masters" element={<ImageManagement />} />
            <Route path="/setup" element={<SetupPage />} />
          </Routes>

          {/* Loading Indicator */}
          {loading && (
            <div className="fixed inset-0 bg-gray-500 bg-opacity-50 flex items-center justify-center z-[70]">
              <div className="animate-spin rounded-full h-16 w-16 border-t-4 border-b-4 border-blue-500"></div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default App;
