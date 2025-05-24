import { invoke } from '@tauri-apps/api/core';
import { Eye, RefreshCw } from 'lucide-react';
import { useEffect, useState, useMemo, useCallback } from 'react';
import { Button, Card } from '../ui';
import { useNotification } from '../../contexts/NotificationContext';
import ServiceConfigModal from './ServiceConfigModal';
import ZfsPoolCard from './ZfsPoolCard';
import { useAppStore } from '../../store/useAppStore';

export const ServiceManagement = () => {
  const { services, fetchData, loading } = useAppStore();
  const [isViewConfigModalOpen, setIsViewConfigModalOpen] = useState(false);
  const [configContent, setConfigContent] = useState('');
  const [configTitle, setConfigTitle] = useState('');
  const [configLoading, setConfigLoading] = useState(false);
  const { showNotification } = useNotification();
  const [zpoolStats, setZpoolStats] = useState(null);

  // --- Service Actions ---
  const handleServiceRestart = useCallback(async (serviceKey) => {
    await invoke('control_service', {
      serviceKey: serviceKey,
      req: { action: 'restart' }
    }).then((response) => {
      if (response.message) showNotification(response.message, 'success');
    }).catch((error) => showNotification(error, 'error',));
  }, [showNotification]);

  const handleViewConfig = useCallback(async (serviceKey, serviceName) => {
    setConfigTitle(`Configuration: ${serviceName}`);
    setConfigContent('');
    setIsViewConfigModalOpen(true);
    setConfigLoading(true);
    try {
      const configData = await invoke('get_service_config', { serviceKey });
      if (configData && typeof configData === 'object' && 'text' in configData) {
        setConfigContent(configData.text);
      } else if (typeof configData === 'object') {
        setConfigContent(JSON.stringify(configData, null, 2));
      } else {
        setConfigContent(String(configData));
      }
    } catch (error) {
      setConfigContent(`Error loading configuration:\n${error.message}`);
    } finally {
      setConfigLoading(false);
    }
  }, []);

  useEffect(() => {
    invoke("get_zpool_stats")
      .then((stats) => {
        setZpoolStats(stats);
      })
  }, []);

  const memoizedServices = useMemo(() => Object.entries(services), [services]);

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6 md:mb-8">
      {memoizedServices.length > 0 ? memoizedServices.map(([key, service]) => (
        <Card key={key} title={service.name} className="flex-1" titleClassName="text-base md:text-lg">
          <div className="flex items-center justify-between">
            <span className={`px-2 py-0.5 rounded-full text-xs font-semibold capitalize ${service.status === 'active' || service.status === 'running' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' :
              service.status === 'inactive' || service.status === 'stopped' ? 'bg-gray-200 text-gray-800 dark:bg-gray-700 dark:text-gray-300' :
                'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
              }`}>
              {service.status}
            </span>
            <div className="flex space-x-1">
              <Button onClick={() => handleViewConfig(key, service.name)} variant="ghost" size="icon" className="h-7 w-7" title={`View Config for ${service.name}`}>
                <Eye className="h-4 w-4 text-gray-500" />
              </Button>
              {(key !== 'zfs') && (
                <Button onClick={() => handleServiceRestart(key)} variant="ghost" size="icon" className="h-7 w-7" title={`Restart ${service.name}`}>
                  <RefreshCw className="h-4 w-4 text-blue-500" />
                </Button>
              )}
            </div>
          </div>
        </Card>
      )) : !loading && <p className="text-gray-500 col-span-full">Could not load service status.</p>}
      <ZfsPoolCard title="ZFS Pool Usage" />
      <ServiceConfigModal loading={configLoading} open={isViewConfigModalOpen} setOpen={setIsViewConfigModalOpen} content={configContent} />
    </div>
  );
};

export default ServiceManagement;
