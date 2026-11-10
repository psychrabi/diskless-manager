import { PlayCircle, StopCircle } from "lucide-react";
import { useCallback, useState } from "react";
import { useShallow } from "zustand/shallow";
import { useServiceManager } from "../../hooks/useServiceManager";
import { useAppStore } from "@/store/useAppStore";
import { useConfirm } from "@/contexts/confirmDialog";
import { Button, PageHeader } from "@/components/ui";
import BootScript from "./BootScript";
import FirewallBanner from "./FirewallBanner";
import ServiceConfigModal from "./ServiceConfigModal";
import ServicesList from "./ServicesList";

const ServiceManagement = () => {
  const { fetchServiceConfig, startAllServices, stopAllServices } =
    useServiceManager();
  const confirm = useConfirm();
  const services = useAppStore(
    useShallow((state) => ({ services: state.services })),
  ).services;
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

  /**
   * Stopping the whole stack is the widest-blast-radius action in the app:
   * it severs PXE, DHCP/TFTP, the iSCSI target, and file sharing for every
   * client at once, so it is gated and names what it is about to take down.
   */
  const handleStopAll = useCallback(async () => {
    const running = (services || []).filter((service) => service.running);
    const names = running.map((service) => service.display_name || service.name);
    const confirmed = await confirm({
      title: "Stop all services",
      description: `This stops ${
        names.length ? names.join(", ") : "every managed service"
      }. Clients that are booting or streaming their disk lose their session immediately, and no new client can be provisioned until the services are started again.`,
      confirmText: "Stop all services",
      cancelText: "Cancel",
      confirmVariant: "destructive",
      size: "2xl",
    });
    if (!confirmed) return;
    await handleGlobalAction("stop", stopAllServices);
  }, [confirm, services, stopAllServices]);

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
    <div className="flex flex-col gap-5">
      <PageHeader
        title="Services"
        description="Every daemon behind boot, storage, and file sharing — with live control."
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
              onClick={handleStopAll}
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
