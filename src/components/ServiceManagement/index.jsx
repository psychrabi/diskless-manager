import { PlayCircle, Server, StopCircle } from "lucide-react";
import { useCallback, useState } from "react";
import { useServiceManager } from "../../hooks/useServiceManager";
import { Button, Card } from "../ui";
import BootScript from "./BootScript";
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
    <Card
      title="Service Management"
      icon={Server}
      className="bg-base-300"
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
    >
      <div className="min-h-[calc(100vh-13rem)] space-y-4">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
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
    </Card>
  );
};

export default ServiceManagement;
