import React from 'react';
import { RefreshCw, PowerOff } from 'lucide-react';
import { useServiceManager } from '../hooks/useServiceManager';
import { Card, Button } from '../components/ui';

export const ServiceManagement = ({ services, refresh, loading }) => {
  const { handleServiceAction } = useServiceManager(services, refresh);

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      {Object.entries(services).map(([key, service]) => (
        <Card key={key} title={service.name} className="flex-1" titleClassName="text-base md:text-lg">
          <div className="flex items-center justify-between">
            <span className={`px-2 py-0.5 rounded-full text-xs font-semibold ${
              service.status === 'active' 
                ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200'
                : 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-200'
            }`}>
              {service.status}
            </span>
            <div className="flex space-x-1">
              <Button
                onClick={() => handleServiceAction(key, 'restart')}
                variant="outline"
                size="icon"
                className="h-7 w-7"
                title={service.status === 'active' ? 'Restart' : 'Start'}
                disabled={loading}
              >
                <RefreshCw className="h-4 w-4" />
              </Button>
              {service.status === 'active' && (
                <Button
                  onClick={() => handleServiceAction(key, 'stop')}
                  variant="outline"
                  size="icon"
                  className="h-7 w-7"
                  title="Stop"
                  disabled={loading}
                >
                  <PowerOff className="h-4 w-4" />
                </Button>
              )}
            </div>
          </div>
        </Card>
      ))}
    </div>
  );
};

export default ServiceManagement;
