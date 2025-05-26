import { use, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { useAppStore } from "../store/useAppStore";

export default function SetupPage() {
  const { navigate } = useNavigate();
  const [disks, setDisks] = useState([]);
  
  const [poolExists, setPoolExists] = useState(null);

  const [selectedDisk, setSelectedDisk] = useState("");
  const [poolName, setPoolName] = useState("diskless");
  const [installing, setInstalling] = useState(false);

  const { services,error, setClients, setServices } = useAppStore();

  useEffect(() => {    
    invoke("list_disks").then(setDisks)
    invoke("zfs_pool_exists", { poolName }).then(setPoolExists);
    invoke("check_services").then(setServices)

    if(poolExists){
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
  const anyServiceNotInstalled = Object.values(services).some(svc => !svc.installed);

  return (
    <div className="max-w-xl mx-auto mt-10 p-6 bg-white rounded shadow">
      <h2 className="text-2xl font-bold mb-4">Initial Setup</h2>
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
        <ul>
         {Object.entries(services).map(([key, svc]) => (
          <li key={key} className="mb-2 flex items-center">
            <span className="flex-1">
              {svc.name} - {svc.installed ? "Installed" : "Not Installed"}
            </span>
            {!svc.installed && (
              <button
                className="px-2 py-1 bg-green-600 text-white rounded"
                disabled={installing === key}
                onClick={() => handleInstallService(svc.name)}
              >
                {installing === key ? "Installing..." : "Install"}
              </button>
            )}
          </li>
        ))}
        </ul>
      </div>
    </div>
  );
}