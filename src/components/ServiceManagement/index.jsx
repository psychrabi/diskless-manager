import { useMemo } from 'react';
import { useAppStore } from '../../store/useAppStore';
import { RAMUsage } from '../RAMUsage';
import ClientOverviewCard from './ClientOverviewCard';
import MasterImageOverviewCard from './MasterImageOverviewCard';
import ServerInfoCard from './ServerInfoCard';
import { ServiceCard } from '../ui';
import ServiceConfigModal from './ServiceConfigModal';
import ZfsPoolCard from './ZfsPoolCard';


const ServiceManagement = () => {
  const { services } = useAppStore();

  return (
    <div className="space-y-6">
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        <ServerInfoCard />
        <RAMUsage />
        <ZfsPoolCard title="ZFS Pool Usage" />
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        {services.map((service, index) => (
          <ServiceCard key={index} service={service} />
        ))}
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
        <ClientOverviewCard />
        <MasterImageOverviewCard />
      </div>
      <ServiceConfigModal />
    </div>
  );
};

export default ServiceManagement;
