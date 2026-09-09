import { Card as ShadcnCard, CardContent, CardDescription, CardTitle } from "@/components/ui/card";
import { useAppStore } from "@/store/useAppStore";
import { HardDrive, PlusCircle } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, PageHeader } from "@/components/ui";
import DiskFormModal from "./DiskFormModal";
import DiskTable from "./DiskTable";

export default function DisksManagement() {
  const fetchDisks = useAppStore((state) => state.fetchDisks);
  const zpools = useAppStore((state) => state.zpools);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [selectedPool, setSelectedPool] = useState("");
  const { datasets, fetchDatasets } = useAppStore();

  const handleDiskFormModalOpen = useCallback(() => {
    setIsModalOpen(true);
  }, []);

  // Set default pool when zpools are loaded
  const poolInitialized = useRef(false);
  useEffect(() => {
    if (zpools.length > 0 && !poolInitialized.current) {
      poolInitialized.current = true;
      setSelectedPool(zpools[0]);
    }
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
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Disk Management"
        description="Manage diskless boot server disks"
        actions={
          datasets.length > 0 && (
            <Button
              variant="primary"
              onClick={() => handleDiskFormModalOpen()}
              icon={PlusCircle}
            >
              Add Disk
            </Button>
          )
        }
      />
      <div className="min-h-[50vh]">
        {datasets.length === 0 ? (
          <ShadcnCard className="shadow-sm border border-border/50">
            <CardContent className="flex flex-col gap-4 items-center text-center p-12">
              <div className="w-20 h-20 bg-muted rounded-full flex items-center justify-center text-4xl mb-4">
                <HardDrive />
              </div>
              <CardTitle className="text-2xl mb-2">No Disks Available</CardTitle>
              <CardDescription className="max-w-md mb-6">
                Create your first Boot image for clients to boot from.
              </CardDescription>
              <Button
                variant="primary"
                onClick={handleDiskFormModalOpen}
              >
                Add Disk
              </Button>
            </CardContent>
          </ShadcnCard>
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
    </div>
  );
}
