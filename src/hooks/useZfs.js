import * as api from "@/api/commands";
import { useToastStore } from "@/store/useToastStore";
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
        const res = await api.listDatasets(pool);
        setDatasets(res || []);
      } catch (e) {
        error(
          `Failed to list datasets: ${e.message || "An unknown error occurred"}`
        );
        console.error(String(e));
      } finally {
        setLoading(false);
      }
    },
    [error]
  );

  const createDataset = useCallback(
    async (data) => {
      try {
        await api.createZfsDataset({
          zpool: data.zpool,
          name: data.name,
          usage_type: data.usage_type,
          size: data.size ?? "",
        });
        success(`Dataset ${data.name} created successfully.`);
        return true;
      } catch (e) {
        error(
          `Failed to create dataset: ${
            e.message || "An unknown error occurred"
          }`
        );
        return false;
      }
    },
    [success, error]
  );

  const deleteDataset = useCallback(
    async (name) => {
      try {
        const response = await api.deleteZfsDataset(name, true);
        if (response.message) success(response.message);
        return true;
      } catch (e) {
        error(
          `Failed to delete disk: ${e.error || "An unknown error occurred"}`
        );
        return false;
      }
    },
    [success, error]
  );

  return {
    datasets,
    loading,
    setDatasets,
    fetchDatasets,
    createDataset,
    deleteDataset,
  };
};
