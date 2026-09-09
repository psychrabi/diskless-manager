import { useAppStore } from "@/store/useAppStore";
import { useShallow } from "zustand/shallow";
import { CardDescription } from "@/components/ui/card";
import ServiceCard from "./ServiceCard";

const ServicesList = ({ onViewConfig }) => {
  const { services, startService, stopService, restartService, enableServiceBoot, disableServiceBoot } = useAppStore(
    useShallow((state) => ({
      services: state.services,
      startService: state.startService,
      stopService: state.stopService,
      restartService: state.restartService,
      enableServiceBoot: state.enableServiceBoot,
      disableServiceBoot: state.disableServiceBoot,
    })),
  );

  return services?.length === 0 ? (
    <CardDescription>No services available</CardDescription>
  ) : (
    services?.map((service) => (
      <ServiceCard
        key={service.name}
        service={service}
        onViewConfig={onViewConfig}
        startService={startService}
        stopService={stopService}
        restartService={restartService}
        enableServiceBoot={enableServiceBoot}
        disableServiceBoot={disableServiceBoot}
      />
    ))
  );
};

export default ServicesList;
