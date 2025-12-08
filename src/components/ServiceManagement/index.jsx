import { Server } from 'lucide-react';
import { RAMUsage } from '../RAMUsage';
import { Card } from '../ui';
import BootScript from './BootScript';
import ClientOverviewCard from './ClientOverviewCard';
import MasterImageOverviewCard from './MasterImageOverviewCard';
import ServerInfoCard from './ServerInfoCard';
import ServiceConfigModal from './ServiceConfigModal';
import ServicesList from './ServicesList';
import ZfsPoolCard from './ZfsPoolCard';


const ServiceManagement = () => {
  return (
    <Card title="Dashboard" icon={Server} className='bg-base-300 '>
      <div className="min-h-[calc(100vh-13rem)] space-y-4">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          <ServerInfoCard />
          <RAMUsage />
          <ZfsPoolCard title="ZFS Pool Usage" />
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <ServicesList />
          <BootScript />
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          <ClientOverviewCard />
          <MasterImageOverviewCard />
        </div>
        <ServiceConfigModal />
      </div>
    </Card>
  );
};

export default ServiceManagement;
