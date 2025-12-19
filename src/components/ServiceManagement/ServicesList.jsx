import { useAppStore } from "@/store/useAppStore";
import { useShallow } from "zustand/shallow";
import ServiceCard from "./ServiceCard";

const ServicesList = ({ onViewConfig }) => {
  const { services, startService, stopService, restartService } = useAppStore(
    useShallow((state) => ({
      services: state.services,
      startService: state.startService,
      stopService: state.stopService,
      restartService: state.restartService,
    })),
  );

  return services?.length === 0 ? (
    <div className="text-sm text-muted">No services available</div>
  ) : (
    services?.map((service) => (
      <ServiceCard
        key={service.name}
        service={service}
        onViewConfig={onViewConfig}
        startService={startService}
        stopService={stopService}
        restartService={restartService}
      />
    ))
  );
};

export default ServicesList;
