import { useNotification } from '@/contexts/notification';
import { invoke } from "@tauri-apps/api/core";
import { TableConfig } from "lucide-react";
import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card, Input, Select, Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../ui";

const Setup = () => {
  const navigate = useNavigate();
  const [disks, setDisks] = useState([]);
  const [poolExists, setPoolExists] = useState(null);
  const [installing, setInstalling] = useState('');
  const { services, setServices } = useAppStore();
  const { poolName } = useAppStore();
  const { showNotification } = useNotification();

  const {
    register,
    handleSubmit,
    formState: { isSubmitting }
  } = useForm();

  // Fetch initial data and re-check zpool when poolName changes
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const d = await invoke('list_disks');
        if (!cancelled) setDisks(d);
      } catch (e) {
        showNotification('error', 'Failed to list disks', e.message || 'An unknown error occurred');
        console.warn('list_disks failed:', e);
        if (!cancelled) setDisks([]);
      }

      try {
        const exists = await invoke('zfs_pool_exists');
        if (!cancelled) setPoolExists(exists);
      } catch (e) {
        showNotification('error', 'Failed to check ZFS pool existence', e.message || 'An unknown error occurred');
        console.warn('zfs_pool_exists failed:', e);
        if (!cancelled) setPoolExists(false);
      }

      try {
        const updated = await invoke('check_package_status');
        const list = Array.isArray(updated) ? updated : (updated ? Object.values(updated) : []);
        if (!cancelled) setServices(list);
      } catch (e) {
        showNotification('error', 'Failed to check package status', e.message || 'An unknown error occurred');
        console.warn('check_package_status failed:', e);
      }
    })();
    return () => { cancelled = true; };
  }, [poolName, setServices, showNotification]);

  // Navigate to dashboard when everything is ready
  useEffect(() => {
    const allServicesInstalled = !(services || []).some((svc) => !svc?.installed);
    // only go to dashboard when services are installed AND zpool exists
    if (allServicesInstalled && poolExists) {
      navigate('/');
    }
  }, [services, navigate, poolExists]);

  const handleCreatePool = (async (data) => {

    try {
      await invoke('create_zfs_pool', { name: data.name, disk: data.disk });
      showNotification('success', 'ZFS Pool Created', `ZFS pool ${data.name} created successfully.`);
    } catch (e) {
      showNotification('error', 'Failed to create ZFS pool', e.message || 'An unknown error occurred');
      console.error('Failed to create ZFS pool:', e);
    }
  });

  const handleInstallService = async (service) => {
    setInstalling(service);
    try {
      await invoke('install_service', { service, token: localStorage.getItem('authToken') || '' });
      showNotification('success', 'Service Installed', `Service ${service} installed successfully.`);
      const updated = await invoke('check_package_status');
      const list = Array.isArray(updated) ? updated : (updated ? Object.values(updated) : []);
      setServices(list);
    } catch (e) {
      showNotification('error', 'Failed to install service', e.message || 'An unknown error occurred');
      console.error('Failed to install service:', e);
    } finally {
      setInstalling('');
    }
  };

  const isZfsInstalled = services?.some(s => s.name === 'zfsutils-linux' && s.installed);
  const zfsNotInstalledMessage = !isZfsInstalled && (
    <div className="alert alert-warning  relative my-4 transition-opacity duration-300">
      <span>zfsutils-linux package is not installed. Please install it to create ZFS pools.</span>
    </div>
  );

  return (
    <Card title="Initial Setup" icon={TableConfig}>
      <Card title="ZFS Pool Create" className={`bg-base-300 ${poolExists ? 'hidden' : ''}`}>
        <form onSubmit={handleSubmit(handleCreatePool)}>
          <div className="space-y-4">
            <Select register={register('disk')} disabled={!isZfsInstalled} label="Select a disk to create ZFS Pool:">
              <option value="">-- Select Disk --</option>
              {disks.map(disk => (
                <option key={disk.name} value={disk.name}> {disk.name} ({disk.size}) </option>
              ))}
            </Select>
            <Input register={register('name')} disabled={!isZfsInstalled} placeholder="ZFS Pool Name" label="ZFS Pool Name" />
            <Button variant="primary" type="submit" disabled={!isZfsInstalled || isSubmitting}>
              Create ZFS Pool
            </Button>
          </div>
        </form>
      </Card>
      {zfsNotInstalledMessage}
      <Card title="Requires Services" className="bg-base-200 mt-4">
        <h3 className="font-semibold mb-2">Required Services</h3>
        <div className="overflow-x-auto">
          <Table className='border border-base-300'>
            <TableHeader>
              <TableRow>
                <TableHead>Service Name</TableHead>
                <TableHead>Version</TableHead>
                <TableHead>Installed</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Action</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {(services || []).map((svc) => (
                <TableRow key={svc.name}>
                  <TableCell>{svc.name}</TableCell>
                  <TableCell>{svc.version}</TableCell>
                  <TableCell>{svc.installed ? "Installed" : "Not Installed"}</TableCell>
                  <TableCell>{svc.running ? "Running" : "Stopped"}</TableCell>
                  <TableCell>
                    {!svc.installed ? (
                      <button className="btn btn-success btn-sm" disabled={installing === svc.name} onClick={() => handleInstallService(svc.name)}>
                        {installing === svc.name ? "Installing..." : "Install"}
                      </button>
                    ) : "No Action Required"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </Card>
    </Card>
  );
}

export default Setup;