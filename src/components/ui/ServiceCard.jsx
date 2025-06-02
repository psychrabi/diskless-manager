import { Eye, RefreshCw } from "lucide-react";
import { Button, Card } from ".";

export const ServiceCard = ({ key, service, handleViewConfig, handleServiceRestart }) => {

  return (<Card key={key} title={service.name} className="flex-1" titleClassName="text-base md:text-lg">
    <div className="flex items-center justify-between">
      <span className={`px-2 py-0.5 rounded-full text-xs font-semibold capitalize ${service.status === 'active' || service.status === 'running' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' :
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
        {(key !== 'zfs') && (
          <Button onClick={() => handleServiceRestart(key)} variant="ghost" size="icon" className="h-7 w-7" title={`Restart ${service.name}`}>
            <RefreshCw className="h-4 w-4 text-blue-500" />
          </Button>
        )}
      </div>
    </div>
  </Card>)
}