import { useToastStore } from "@/store/useToastStore";
import { invoke } from "@tauri-apps/api/core";
import { TableConfig } from "lucide-react";
import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "../../store/useAppStore";
import {
  Button,
  Card,
  Input,
  Select,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "../ui";

const Setup = () => {
  const navigate = useNavigate();
  const [disks, setDisks] = useState([]);
  const [poolExists, setPoolExists] = useState(null);
  const [installing, setInstalling] = useState("");
  const { poolName } = useAppStore();
  const { error, success } = useToastStore();

  const { services, setServices, dependencies } = useAppStore(
    useShallow((state) => ({
      services: state.services,
      setServices: state.setServices,
      dependencies: state.dependencies,
    }))
  );

  const {
    register,
    handleSubmit,
    formState: { isSubmitting },
  } = useForm();

  // Fetch initial data and re-check zpool when poolName changes
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const d = await invoke("list_disks");
        if (!cancelled) setDisks(d);
      } catch (e) {
        error(
          `Failed to list disks ${e.message || "An unknown error occurred"}`
        );
        console.warn("list_disks failed:", e);
        if (!cancelled) setDisks([]);
      }

      try {
        const exists = await invoke("zfs_pool_exists");
        if (!cancelled) setPoolExists(exists);
      } catch (e) {
        error(
          `Failed to check ZFS pool existence ${
            e.message || "An unknown error occurred"
          }`
        );
        console.warn("zfs_pool_exists failed:", e);
        if (!cancelled) setPoolExists(false);
      }

      try {
        const updated = await invoke("check_package_status");
        const list = Array.isArray(updated)
          ? updated
          : updated
          ? Object.values(updated)
          : [];
        if (!cancelled) setServices(list);
      } catch (e) {
        error(
          `Failed to check package status ${
            e.message || "An unknown error occurred"
          }`
        );
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [poolName, setServices, error]);

  // Navigate to dashboard when everything is ready
  useEffect(() => {
    const allServicesInstalled = !(services || []).some(
      (svc) => !svc?.installed
    );
    // only go to dashboard when services are installed AND zpool exists
    if (allServicesInstalled && poolExists) {
      navigate("/");
    }
  }, [services, navigate, poolExists]);

  const handleCreatePool = async (data) => {
    try {
      await invoke("create_zfs_pool", {
        req: { name: data.name, disk: data.disk },
      });
      success(`ZFS pool ${data.name} created successfully.`);
    } catch (e) {
      error(
        `Failed to create ZFS pool ${e.message || "An unknown error occurred"}`
      );
    }
  };

  const handleInstallService = async (service) => {
    setInstalling(service);
    try {
      await invoke("install_service", {
        service,
        token: localStorage.getItem("authToken") || "",
      });
      success(`Service ${service} installed successfully.`);
      const updated = await invoke("check_package_status");
      const list = Array.isArray(updated)
        ? updated
        : updated
        ? Object.values(updated)
        : [];
      setServices(list);
    } catch (e) {
      error(
        `Failed to install service ${e.message || "An unknown error occurred"}`
      );
    } finally {
      setInstalling("");
    }
  };

  const isZfsInstalled = services?.some(
    (s) => s.name === "zfsutils-linux" && s.installed
  );
  const zfsNotInstalledMessage = !isZfsInstalled && (
    <div className="alert alert-warning  relative my-4 transition-opacity duration-300">
      <span>
        zfsutils-linux package is not installed. Please install it to create ZFS
        pools.
      </span>
    </div>
  );

  return (
    <Card title="Initial Setup" icon={TableConfig}>
      <Card title="ZFS Pool Create" className={`bg-base-300`}>
        <form onSubmit={handleSubmit(handleCreatePool)}>
          <div className="space-y-4">
            <Select
              register={register("disk")}
              disabled={!isZfsInstalled}
              label="Select a disk to create ZFS Pool:"
            >
              <option value="">-- Select Disk --</option>
              {disks.map((disk) => (
                <option key={disk.name} value={disk.name}>
                  {" "}
                  {disk.name} ({disk.size}){" "}
                </option>
              ))}
            </Select>
            <Input
              register={register("name")}
              disabled={!isZfsInstalled}
              placeholder="ZFS Pool Name"
              label="ZFS Pool Name"
            />
            <Button
              variant="primary"
              type="submit"
              disabled={!isZfsInstalled || isSubmitting || poolExists}
            >
              Create ZFS Pool
            </Button>
          </div>
        </form>
      </Card>
      {zfsNotInstalledMessage}
      <Card title="System Dependencies" className="bg-base-200 mt-4">
        <div className="overflow-x-auto">
          <Table className="border border-base-300">
            <TableHeader>
              <TableRow>
                <TableHead>Service Name</TableHead>
                <TableHead>Version</TableHead>
                <TableHead>Installed</TableHead>
                <TableHead>Action</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {dependencies.map((svc) => (
                <TableRow key={svc.name}>
                  <TableCell>{svc.name}</TableCell>
                  <TableCell>{svc.version}</TableCell>
                  <TableCell>
                    {svc.installed ? "Installed" : "Not Installed"}
                  </TableCell>
                  <TableCell>
                    {!svc.installed && (
                      <button
                        className="btn btn-success btn-sm"
                        disabled={installing === svc.name}
                        onClick={() => handleInstallService(svc.name)}
                      >
                        {installing === svc.name ? "Installing..." : "Install"}
                      </button>
                    )}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </Card>
    </Card>
  );
};

export default Setup;
