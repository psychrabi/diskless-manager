import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card, Input, Select } from "../ui";
import { TableConfig } from "lucide-react";


const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full">{children}</table></div>;
const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-base-100 ${className}`}>{children}</thead>;
const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-base-300 transition-colors hover:bg-base-300 ${className}`}>{children}</tr>;
const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 align-middle font-bold text-base-content/60 ${className} text-left`}>{children}</th>;
const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className}`}>{children}</td>;

const Setup = () => {
  const navigate = useNavigate();
  const [disks, setDisks] = useState([]);
  const [poolExists, setPoolExists] = useState(null);
  const [selectedDisk, setSelectedDisk] = useState("");
  const [poolName, setPoolName] = useState("diskless");
  const [installing, setInstalling] = useState(false);
  const { services } = useAppStore();

  useEffect(() => {
    invoke("list_disks").then(setDisks)
    invoke("zfs_pool_exists", { poolName }).then(setPoolExists);

    // Check if any services are not installed
    const hasUninstalledServices = Object.values(services).some(
      service => !service.installed
    );

    if (!hasUninstalledServices) {
      navigate('/');
    }
  }, [services, navigate]);

  const handleCreatePool = async () => {
    await invoke("create_zfs_pool", { name: poolName, disk: selectedDisk });
  };

  const handleInstallService = async (service) => {
    setInstalling(true);
    await invoke("install_service", { service });
    const updated = await invoke("check_package_status");
    setServices(updated);
    setInstalling(false);
  };

  return (
    <Card title="Initial Setup" icon={TableConfig} className="">
      <Card title="ZFS Pool Create" className="bg-base-200">
        <form>
          <div className="space-y-4">
            <Select defaultValue={selectedDisk} onChange={e => setSelectedDisk(e.target.value)} label="Select a disk to create ZFS Pool:">
              <option value="">-- Select Disk --</option>
              {disks.map(disk => (
                <option key={disk.name} value={disk.name}> {disk.name} ({disk.size}) </option>
              ))}
            </Select>
            <Input value={poolName} onChange={e => setPoolName(e.target.value)} placeholder="ZFS Pool Name" label="ZFS Pool Name" />
            <Button variant="primary" type="submit" disabled={!selectedDisk} onClick={handleCreatePool} >
              Create ZFS Pool
            </Button>
          </div>
        </form>
      </Card>
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
              {Object.entries(services).map(([key, svc]) => (
                <TableRow key={key}>
                  <TableCell>{svc.name}</TableCell>
                  <TableCell>{svc.version}</TableCell>
                  <TableCell>{svc.installed ? "Installed" : "Not Installed"}</TableCell>
                  <TableCell>{svc.running ? "Running" : "Stopped"}</TableCell>
                  <TableCell>
                    {!svc.installed ? (
                      <button className="btn btn-success btn-sm" disabled={installing === key} onClick={() => handleInstallService(svc.name)}>
                        {installing === key ? "Installing..." : "Install"}
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