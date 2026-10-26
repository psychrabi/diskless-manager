import { useEffect, useState } from "react";
import { useToastStore } from "@/store/useToastStore";
import { useAuth } from "@/contexts/auth";
import { Button } from "@/components/ui";
import { Input } from "@/components/ui/input";
import { Card, CardTitle, CardDescription } from "@/components/ui/card";
import { useConfirm } from "@/contexts/confirmDialog";
import { apiRequest, getAuthToken } from "@/api/client";
import { useNavigate } from "react-router-dom";

export function DriverCatalog() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [drivers, setDrivers] = useState([]);
  const [loading, setLoading] = useState(true);
  const [uploading, setUploading] = useState(false);
  const [importProgress, setImportProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const { success, error: toastError } = useToastStore();
  const confirm = useConfirm();
  const driverImportUrl = "/api/pxe/network-drivers";

  useEffect(() => {
    async function loadDrivers() {
      setLoading(true);
      setError(null);
      try {
        const result = await apiRequest("/api/pxe/network-drivers");
        if (Array.isArray(result)) {
          setDrivers(result);
        } else {
          setDrivers([]);
          throw new Error("Unexpected driver list format");
        }
      } catch (e: any) {
        setError(
          e?.message || "Failed to load network drivers"
        );
        toastError("Driver Catalog", e?.message || "Failed to load network drivers");
      } finally {
        setLoading(false);
      }
    }

    loadDrivers();
  }, [toastError]);

  const handleImport = async (file: File) => {
    if (user?.role !== "admin") {
      toastError("Access Denied", "Admin role required to import drivers");
      return;
    }

    setUploading(true);
    setError(null);
    setImportProgress(0);

    const formData = new FormData();
    formData.append("file", file);

    try {
      const xhr = new XMLHttpRequest();
      xhr.open("POST", driverImportUrl, true);
      const token = getAuthToken();
      if (token) xhr.setRequestHeader("Authorization", `Bearer ${token}`);
      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable) {
          const percent = Math.round((e.loaded / e.total) * 100);
          setImportProgress(percent);
        }
      };
      xhr.onload = async () => {
        setUploading(false);
        if (xhr.status >= 200 && xhr.status < 300) {
          const result = JSON.parse(xhr.responseText);
          // Refresh driver list
          const result2 = await apiRequest(driverImportUrl);
          if (Array.isArray(result2)) {
            setDrivers(result2);
          }
          success(
            "Driver Catalog",
            `Imported driver package: ${result.name || "unknown"} (${
              result.version ? "v" + result.version : ""
            })`
          );
        } else {
          const err = JSON.parse(xhr.responseText);
          toastError("Driver Catalog", err.error || "Import failed");
          setError(err.error || "Import failed");
        }
      };
      xhr.onerror = () => {
        setUploading(false);
        toastError("Driver Catalog", "Network error during import");
        setError("Network error");
      };
      xhr.send(formData);
    } catch (e: any) {
      setUploading(false);
      setError(e?.message || "Import failed");
      toastError("Driver Catalog", e?.message || "Import failed");
    }
  };

  const handleRebuildCatalog = async () => {
    if (user?.role !== "admin") {
      toastError("Access Denied", "Admin role required to rebuild catalog");
      return;
    }

    const ok = await confirm({
      title: "Rebuild Driver Catalog",
      description:
        "This will rebuild the driver catalog from all imported packages. This may take a moment.",
      confirmText: "Rebuild",
      cancelText: "Cancel",
      confirmVariant: "destructive",
    });

    if (!ok) return;

    setUploading(true);
    setError(null);

    try {
      await apiRequest("/api/pxe/network-drivers/rebuild", {
        method: "POST",
      });
      success("Driver Catalog", "Driver catalog rebuilt successfully");
      const result2 = await apiRequest(driverImportUrl);
      if (Array.isArray(result2)) {
        setDrivers(result2);
      }
    } catch (e: any) {
      toastError("Driver Catalog", e?.message || "Failed to rebuild catalog");
      setError(e?.message || "Failed to rebuild catalog");
    } finally {
      setUploading(false);
    }
  };

  if (user?.role !== "admin") {
    return (
      <Card className="p-6">
        <CardTitle>Driver Catalog</CardTitle>
        <CardDescription>Login as administrator to view and manage drivers</CardDescription>
        <Button
          variant="primary"
          onClick={() => navigate("/login")}
          className="mt-2"
        >
          Login as Admin
        </Button>
      </Card>
    );
  }

  return (
    <Card className="p-6">
      <CardTitle>Network Driver Catalog</CardTitle>
      <CardDescription>
        Imported driver packages for Windows network-boot image preparation.
        {drivers.length > 0 && ` ${drivers.length} package${drivers.length !== 1 ? "s" : ""} loaded`}
      </CardDescription>

      {/* Driver list */}
      {loading && <p className="mt-2 text-sm text-muted-foreground">Loading drivers...</p>}

      {!loading && drivers.length > 0 && (
        <div className="overflow-x-auto mt-4">
          <table className="w-full text-sm text-left text-muted-rounded">
            <thead className="text-xs text-muted-foreground border-b border-border/60">
              <tr>
                <th className="p-3 font-medium">Name</th>
                <th className="p-3 font-medium">Version</th>
                <th className="p-3 font-medium">Architectures</th>
                <th className="p-3 font-medium">Hardware IDs</th>
              </tr>
            </thead>
            <tbody>
              {drivers.map((driver) => (
                <tr key={driver.id} className="border-b border-border/50 hover:bg-muted/30">
                  <td className="p-3">{driver.name || "Unnamed"}</td>
                  <td className="p-3">
                    {driver.version
                      ? `v${driver.version}`
                      : <span className="text-xs text-muted-foreground">unknown</span>}
                  </td>
                  <td className="p-3">
                    {driver.architectures?.length > 0
                      ? driver.architectures.join(", ")
                      : <span className="text-xs text-muted-foreground">—</span>}
                  </td>
                  <td className="p-3">
                    {driver.hardware_ids?.length > 0
                      ? driver.hardware_ids.slice(0, 3).join(" · ")
                        + (driver.hardware_ids.length > 3
                          ? ` · +${driver.hardware_ids.length - 3}`
                          : "")
                      : <span className="text-xs text-muted-foreground">—</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {!loading && drivers.length === 0 && (
        <p className="mt-2 text-sm text-muted-foreground">
          No imported network driver packages are available.
        </p>
      )}

      {/* Upload section */}
      <div className="mt-6">
        <CardTitle>Upload Driver Package</CardTitle>
        <CardDescription>
          Upload a ZIP archive containing INF files and driver packages.
          Maximum size: 512 MiB
        </CardDescription>

        <div className="mt-3">
          <Input
            type="file"
            accept=".zip"
            className="w-full"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) {
                handleImport(file);
              }
            }}
          />
        </div>

        {error && <p role="alert" className="mt-2 text-sm text-destructive">{error}</p>}
        {uploading && (
          <div className="mt-3">
            <p className="text-sm text-muted-foreground">
              Import progress: <strong>{importProgress}%</strong>
            </p>
          </div>
        )}
      </div>

      {/* Catalog rebuild */}
      {user?.role === "admin" && (
        <div className="mt-6">
          <Button
            variant="outline"
            size="sm"
            className="mt-2 w-full"
            onClick={handleRebuildCatalog}
          >
            Rebuild Driver Catalog
          </Button>
          <p className="mt-2 text-sm text-muted-foreground">
            Rebuild the catalog from all imported packages. Required after driver imports
            to make them available for image preparation.
          </p>
        </div>
      )}
    </Card>
  );
}
