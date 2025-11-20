import { useServiceManager } from "@/hooks/useServiceManager"
import { Eye, Play, Power, RefreshCw } from "lucide-react"
import { Button, Card } from "../ui"
import { useAppStore } from "@/store/useAppStore";

const ServicesList = () => {
  const services = useAppStore((state) => state.services);
  const { handleServiceAction, handleServiceConfigView } = useServiceManager()
  const list = Array.isArray(services) ? services : (services ? Object.values(services) : []);

  return (
    list.length === 0 ? (
      <div className="text-sm text-muted">No services available</div>
    ) : (
      list.map((service, index) => (
        <Card key={service.service || index} title={service.name} className="flex-1" titleClassName="text-base md:text-lg">
          <div className="flex items-center justify-between">
            <span className={`px-2 py-0.5 rounded-full text-xs font-semibold capitalize ${service.running ? 'dark:bg-green-300 dark:text-green-900 bg-green-900 text-green-300' :
              'bg-base-300 text-base-content'
              }`}>
              {service.running ? 'Running' : 'Stopped'}
            </span>
            <div className="flex space-x-1">
              <Button onClick={() => handleServiceConfigView(service.service, service.name)} variant="ghost" size="icon" className="h-7 w-7" title={`View Config for ${service.service}`}>
                <Eye className="h-4 w-4 text-base-content" />
              </Button>
              {(service.service !== 'zfs') && (
                <>
                  {!service.running && (
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
      ))
    )
  );
}

export default ServicesList;