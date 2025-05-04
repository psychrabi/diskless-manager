import React, { useState } from 'react';
import { RefreshCw, Eye } from 'lucide-react';
import { useServiceManager } from '../hooks/useServiceManager';
import { Card, Button, Modal } from '../components/ui';
import { apiRequest, handleApiAction } from '../utils/apiRequest';

export const ServiceManagement = ({ services, refresh, loading, setActionStatus }) => {
  const { handleServiceAction } = useServiceManager(services, refresh);
  const [isViewConfigModalOpen, setIsViewConfigModalOpen] = useState(false); // New state for view config modal
  const [configContent, setConfigContent] = useState('');
  const [configTitle, setConfigTitle] = useState('');
  const [configLoading, setConfigLoading] = useState(false);


    // --- Service Actions ---
    const handleServiceRestart = (serviceKey) => {
      handleApiAction(
           () => apiRequest(`/services/${serviceKey}/control`, 'POST', { action: 'restart' }),
           `Restart command sent to service ${serviceKey}.`,
           `Failed to send restart command to service ${serviceKey}`
       );
 };

    const handleViewConfig = async (serviceKey, serviceName) => {
        setConfigTitle(`Configuration: ${serviceName}`);
        setConfigContent(''); // Clear previous content
        setIsViewConfigModalOpen(true);
        setConfigLoading(true);
        try {
            // Use responseType 'text' for config files
            const configData = await apiRequest(`/services/${serviceKey}/config`, 'GET', null, 'text');
            setConfigContent(configData);
        } catch (error) {
            setConfigContent(`Error loading configuration:\n${error.message}`);
        } finally {
            setConfigLoading(false);
        }
    };
  

  return (
     <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6 md:mb-8">
          {Object.entries(services).length > 0 ? Object.entries(services).map(([key, service]) => (
              <Card key={key} title={service.name} className="flex-1" titleClassName="text-base md:text-lg">
                  <div className="flex items-center justify-between">
                       <span className={`px-2 py-0.5 rounded-full text-xs font-semibold capitalize ${
                           service.status === 'active' || service.status === 'running' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' :
                           service.status === 'inactive' || service.status === 'stopped' ? 'bg-gray-200 text-gray-800 dark:bg-gray-700 dark:text-gray-300' :
                           'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200' // error, failed, degraded etc.
                       }`}>
                          {service.status}
                      </span>
                      {/* Service Action Buttons */}
                      <div className="flex space-x-1">
                          <Button onClick={() => handleViewConfig(key, service.name)} variant="ghost" size="icon" className="h-7 w-7" title={`View Config for ${service.name}`}>
                              <Eye className="h-4 w-4 text-gray-500" />
                          </Button>
                          <Button onClick={() => handleServiceRestart(key)} variant="ghost" size="icon" className="h-7 w-7" title={`Restart ${service.name}`}>
                              <RefreshCw className="h-4 w-4 text-blue-500" />
                          </Button>
                      </div>
                  </div>
              </Card>
          )) : !loading && <p className="text-gray-500 col-span-full">Could not load service status.</p> }
                 {/* View Config Modal */}
                 <Modal isOpen={isViewConfigModalOpen} onClose={() => setIsViewConfigModalOpen(false)} title={configTitle} size="2xl">
                      {configLoading ? (
                          <div className="flex justify-center items-center h-40">
                              <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-blue-500"></div>
                          </div>
                      ) : (
                          <pre className="bg-gray-100 dark:bg-gray-900 p-4 rounded-md text-xs overflow-auto max-h-[70vh]">
                              <code>{configContent}</code>
                          </pre>
                      )}
                       <div className="mt-4 flex justify-end">
                            <Button variant="outline" onClick={() => setIsViewConfigModalOpen(false)}>Close</Button>
                        </div>
                 </Modal>
       </div>
  );
};

export default ServiceManagement;
