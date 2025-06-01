import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAppStore } from "../store/useAppStore";
import { Card } from "./ui";

export default function SetupPage() {
  const { navigate } = useNavigate();
  const [disks, setDisks] = useState([]);
  
  const [poolExists, setPoolExists] = useState(null);

  const [selectedDisk, setSelectedDisk] = useState("");
  const [poolName, setPoolName] = useState("diskless");
  const [installing, setInstalling] = useState(false);

  const { services, setServices } = useAppStore();
  const anyServiceNotInstalled = Object.values(services).some(svc => !svc.installed);

  useEffect(() => {    
    invoke("list_disks").then(setDisks)
    invoke("zfs_pool_exists", { poolName }).then(setPoolExists);
    invoke("check_services").then(setServices)

    if(poolExists){
      navigate("/");
    }
    if(!anyServiceNotInstalled){
      navigate("/");
    }
    
  }, []);

  const handleCreatePool = async () => {
    await invoke("create_zfs_pool", { name: poolName, disk: selectedDisk });
  };

  const handleInstallService = async (service) => {
    setInstalling(true);
    await invoke("install_service", { service });
    const updated = await invoke("check_services");
    setServices(updated);
    setInstalling(false);
  };

  // Check if any services are not installed
  console.log(services);



  return (
    <Card title="Initial Setup">
    
    
      
      {!poolExists && (<div className="mb-4">
        <label className="block mb-2">Select disk to create ZFS pool:</label>
        <select
          className="border p-2 w-full"
          value={selectedDisk}
          onChange={e => setSelectedDisk(e.target.value)}
        >
          <option value="">-- Select Disk --</option>
          {disks.map(disk => (
            <option key={disk.name} value={disk.name}>
              {disk.name} ({disk.size})
            </option>
          ))}
        </select>
        <input
          className="border p-2 mt-2 w-full"
          value={poolName}
          onChange={e => setPoolName(e.target.value)}
          placeholder="ZFS Pool Name"
        />
        <button
          className="mt-2 px-4 py-2 bg-blue-600 text-white rounded"
          disabled={!selectedDisk}
          onClick={handleCreatePool}
        >
          Create Pool
        </button>
      </div>)}
      <div>
        <h3 className="font-semibold mb-2">Required Services</h3>
        {!anyServiceNotInstalled && (
          <div className="mb-2 text-green-700">All required services are installed.</div>
        )}
        <div className="overflow-x-auto">
          <table className="table">
            <thead>
              <tr>
                <th>Service Name</th>
                <th>Status</th>
                <th>Action</th>
              </tr>
            </thead>
            <tbody>
              {Object.entries(services).map(([key, svc]) => (
                <tr key={key}>
                  <td>{svc.name}</td>
                  <td>{svc.installed ? "Installed" : "Not Installed"}</td>
                  <td>
                    {!svc.installed && (
                      <button
                        className="btn btn-success btn-sm"
                        disabled={installing === key}
                        onClick={() => handleInstallService(svc.name)}
                      >
                        {installing === key ? "Installing..." : "Install"}
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </Card>
  );
}