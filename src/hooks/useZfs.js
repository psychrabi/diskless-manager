import { useToastStore } from "@/store/useToastStore";
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

export const useZfs = () => {
  const [datasets, setDatasets] = useState([]);
  const [loading, setLoading] = useState(false);
  const { success, error } = useToastStore();

  const fetchDatasets = useCallback(
    async (pool) => {
      if (!pool) {
        setDatasets([]);
        return;
      }
      setLoading(true);
      try {
        const res = await invoke("list_datasets", { zpool: pool });
        setDatasets(res || []);
      } catch (e) {
        error(
          `Failed to list datasets: ${e.message || "An unknown error occurred"}`,
        );
        console.error(String(e));
      } finally {
        setLoading(false);
      }
    },
    [error],
  );

  const createDataset = useCallback(
    async (data) => {
      try {
        await invoke("create_zfs_dataset", {
          req: {
            zpool: data.zpool,
            name: data.name,
            usage_type: data.usage_type,
            size: data.size ?? "",
          },
        });
        success(`Dataset ${data.name} created successfully.`);
        return true;
      } catch (e) {
        error(
          `Failed to create dataset: ${
            e.message || "An unknown error occurred"
          }`,
        );
        return false;
      }
    },
    [success, error],
  );

  const deleteDataset = useCallback(
    async (name) => {
      const token = localStorage.getItem("authToken") || "";
      try {
        const response = await invoke("delete_zfs_dataset", {
          token,
          dataset: name,
          recursive: true,
        });
        if (response.message) success(response.message);
        return true;
      } catch (e) {
        error(`Failed to delete disk: ${e.error || "An unknown error occurred"}`);
        return false;
      }
    },
    [success, error],
  );

  return {
    datasets,
    loading,
    fetchDatasets,
    createDataset,
    deleteDataset,
  };
};
