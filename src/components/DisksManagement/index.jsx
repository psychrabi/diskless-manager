import { useZfs } from "@/hooks/useZfs";
import { useAppStore } from "@/store/useAppStore";
import { HardDrive, PlusCircle } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Button, Card } from "../ui";
import DiskFormModal from "./DiskFormModal";
import DiskTable from "./DiskTable";

export default function DisksManagement() {
  const fetchDisks = useAppStore((state) => state.fetchDisks);
  const zpools = useAppStore((state) => state.zpools);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [selectedPool, setSelectedPool] = useState("");
  const { datasets, fetchDatasets } = useZfs();

  const handleDiskFormModalOpen = useCallback(() => {
    setIsModalOpen(true);
  }, []);

  // Set default pool when zpools are loaded
  useEffect(() => {
    if (zpools.length > 0 && !selectedPool) {
      setSelectedPool(zpools[0]);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [zpools]);

  useEffect(() => {
    if (selectedPool) {
      fetchDatasets(selectedPool);
    }
  }, [selectedPool, fetchDatasets]);

  const refresh = useCallback(() => {
    if (selectedPool) fetchDatasets(selectedPool);
    fetchDisks();
  }, [selectedPool, fetchDatasets, fetchDisks]);

  return (
    <Card
      title="Disk Management"
      icon={HardDrive}
      className="bg-base-300"
      actions={datasets.length > 0 &&
        <Button
          variant="primary"
          onClick={() => handleDiskFormModalOpen()}
          icon={PlusCircle}
        >
          Add Disk
        </Button>
      }
    >
      <div className="min-h-[calc(100vh-15rem)]">
        {datasets.length === 0 ? (
          <div className="card bg-base-100 shadow-xl border border-base-200/50">
            <div className="card-body items-center text-center p-12">
              <div className="w-20 h-20 bg-base-200 rounded-full flex items-center justify-center text-4xl mb-4">
                <HardDrive />
              </div>
              <h2 className="card-title text-2xl mb-2">No Disks Available</h2>
              <p className="text-base-content/60 max-w-md mb-6">
                Create your first Boot image for clients to boot from.
              </p>
              <button className="btn btn-primary" onClick={handleDiskFormModalOpen}>
                Add Disk
              </button>
            </div>
          </div>
        ) : (
          <DiskTable datasets={datasets} onRefresh={refresh} />
        )}
      </div>
      {isModalOpen && (
        <DiskFormModal
          zpools={zpools}
          isOpen={isModalOpen}
          setIsOpen={setIsModalOpen}
          refresh={refresh}
        />
      )}
    </Card>
  );
}
