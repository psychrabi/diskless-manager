import { PlayCircle, StopCircle } from "lucide-react";
import { useCallback, useState } from "react";
import { useServiceManager } from "../../hooks/useServiceManager";
import { Button, PageHeader } from "@/components/ui";
import BootScript from "./BootScript";
import FirewallBanner from "./FirewallBanner";
import ServiceConfigModal from "./ServiceConfigModal";
import ServicesList from "./ServicesList";

const ServiceManagement = () => {
  const { fetchServiceConfig, startAllServices, stopAllServices } =
    useServiceManager();
  const [globalLoading, setGlobalLoading] = useState(null);
  const [modalState, setModalState] = useState({
    isOpen: false,
    serviceKey: "",
    title: "",
    configContent: "",
    loading: false,
    path: "",
  });

  const handleGlobalAction = async (action, fn) => {
    setGlobalLoading(action);
    try {
      await fn();
    } finally {
      setGlobalLoading(null);
    }
  };

  const handleViewConfig = useCallback(
    async (serviceKey, serviceName) => {
      setModalState((prev) => ({
        ...prev,
        isOpen: true,
        loading: true,
        title: `${serviceName} Configuration`,
        serviceKey,
      }));
      try {
        const data = await fetchServiceConfig(serviceKey);
        setModalState((prev) => ({
          ...prev,
          configContent: data?.text || "No service config found",
          path: data?.path || "",
          loading: false,
        }));
      } catch (error) {
        setModalState((prev) => ({
          ...prev,
          configContent: `Error: ${error.message}`,
          loading: false,
        }));
      }
    },
    [fetchServiceConfig]
  );

  const closeModal = useCallback(() => {
    setModalState((prev) => ({ ...prev, isOpen: false }));
  }, []);

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Service Management"
        description="Manage and monitor system services."
        actions={
          <>
            <Button
              icon={PlayCircle}
              variant="success"
              loading={globalLoading === "start"}
              onClick={() => handleGlobalAction("start", startAllServices)}
              title="Start all services"
            >
              Start All
            </Button>
            <Button
              icon={StopCircle}
              variant="destructive"
              loading={globalLoading === "stop"}
              onClick={() => handleGlobalAction("stop", stopAllServices)}
              title="Stop all services"
            >
              Stop All
            </Button>
          </>
        }
      />
      <div className="space-y-4 min-h-[50vh]">
        <FirewallBanner />
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 2xl:grid-cols-4 gap-4">
          <ServicesList onViewConfig={handleViewConfig} />
          <BootScript onViewConfig={handleViewConfig} />
        </div>
        <ServiceConfigModal
          isOpen={modalState.isOpen}
          onClose={closeModal}
          title={modalState.title}
          serviceKey={modalState.serviceKey}
          initialConfig={modalState.configContent}
          initialLoading={modalState.loading}
          path={modalState.path}
        />
      </div>
    </div>
  );
};

export default ServiceManagement;
