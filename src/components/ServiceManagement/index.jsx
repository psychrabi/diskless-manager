import { Eye, Play, Power, RefreshCw } from 'lucide-react';
import { useMemo } from 'react';
import { useServiceManager } from '../../hooks/useServiceManager';
import { useAppStore } from '../../store/useAppStore';
import { RAMUsage } from '../RAMUsage';
import { Button, Card } from '../ui';
import ServiceConfigModal from './ServiceConfigModal';
import ZfsPoolCard from './ZfsPoolCard';
import ServerInfoCard from './ServerInfoCard';

const ServiceManagement = () => {
  const { loading, services, fetchData } = useAppStore();
  
  const { handleServiceAction, handleServiceConfigView } = useServiceManager()

  const memoizedServices = useMemo(() => Object.entries(services), [services]);

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6 md:mb-8">
      {memoizedServices.length > 0 ? memoizedServices.map(([key, service]) => (
        <Card key={key} title={service.name} className="flex-1" titleClassName="text-base md:text-lg">
          <div className="flex items-center justify-between">
            <span className={`px-2 py-0.5 rounded-full text-xs font-semibold capitalize ${service.status === 'active' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' :
              'bg-gray-200 text-gray-800 dark:bg-gray-700 dark:text-gray-300'
              }`}>
              {service.running ? 'Running' : 'Stopped'}
            </span>
            <div className="flex space-x-1">
              <Button onClick={() => handleServiceConfigView(service.service, service.name)} variant="ghost" size="icon" className="h-7 w-7" title={`View Config for ${service.service}`}>
                <Eye className="h-4 w-4 text-white" />
              </Button>
              {(service.service !== 'zfs') && (
                <>
                  {!service.running  && (
                    <Button onClick={() => handleServiceAction(service.service, 'start')} variant="ghost" size="icon" className="h-7 w-7" title={`Start ${service.service}`} disabled={service.running}>
                      <Play className="h-4 w-4 text-green-500" />
                    </Button>
                  )}
                  {service.running && (
                    <>
                      <Button onClick={() => handleServiceAction(service.service, 'stop')} variant="ghost" size="icon" className="h-7 w-7" title={`Stop ${service.service}`} disabled={service.running === 'inactive'}>
                        <Power className="h-4 w-4 text-red-500" />
                      </Button>
                      <Button onClick={() => handleServiceAction(service.service, 'restart')} variant="ghost" size="icon" className="h-7 w-7" title={`Restart ${service.service}`} disabled={service.running === 'inactive'}>
                        <RefreshCw className="h-4 w-4 text-blue-500" />
                      </Button>
                    </>
                  )}
                </>
              )}
            </div>
          </div>
        </Card>
      )) : !loading && <p className="text-gray-500 col-span-full">Could not load service status.</p>}
      <ZfsPoolCard title="ZFS Pool Usage" />
      <RAMUsage />
      <ServerInfoCard />
      <ServiceConfigModal />
    </div>
  );
};

export default ServiceManagement;
