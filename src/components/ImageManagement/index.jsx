import { buttonVariants } from "@/components/ui/button";
import { Card as ShadcnCard, CardContent, CardDescription, CardTitle } from "@/components/ui/card";
import { Download, File, PlusCircle } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useAppStore } from "../../store/useAppStore";
import { Button, PageHeader } from "@/components/ui";
import { scanAndImportImages } from "@/api/modules/images";
import { useToastStore } from "@/store/useToastStore";
import CreateImageModal from "./CreateImageModal";
import { ImagesList } from "./ImagesList";

const ImageManagement = () => {
  // Use a more specific selector to ensure re-renders
  const masters = useAppStore((state) => state.masters);
  const datasets = useAppStore((state) => state.datasets);
  const zpools = useAppStore((state) => state.zpools);
  const fetchMasters = useAppStore((state) => state.fetchMasters);
  const fetchDatasets = useAppStore((state) => state.fetchDatasets);
  const fetchImages = useAppStore((state) => state.fetchImages);
  const { error, success } = useToastStore();
  const [openImageCreateModal, setOpenImageCreateModal] = useState(false);
  const [importing, setImporting] = useState(false);

  // Check if there are any image disks (datasets with org.diskless:type=image)
  const hasImageDisk = datasets.some(
    (dataset) =>
      dataset.disk_type && dataset.disk_type.toLowerCase() === "image"
  );

  const handleCreateImage = () => {
    if (hasImageDisk) {
      setOpenImageCreateModal(true);
    }
  };

  const handleImportExisting = async () => {
    setImporting(true);
    try {
      const result = await scanAndImportImages();
      await Promise.all([fetchMasters(), fetchImages()]);
      const activePool = zpools[0]?.name;
      if (activePool) {
        await fetchDatasets(activePool);
      }
      success(
        "Import Images",
        `Scanned ZFS pool: ${result?.imported_masters ?? 0} image(s) and ${
          result?.imported_snapshots ?? 0
        } snapshot(s) imported.`
      );
    } catch (e) {
      error("Import Images", `Failed to scan for existing images: ${e}`);
    } finally {
      setImporting(false);
    }
  };

  // Set default pool when zpools are loaded
  const poolInitialized = useRef(false);
  useEffect(() => {
    if (zpools.length > 0 && !poolInitialized.current) {
      poolInitialized.current = true;
    }
  }, [zpools]);

  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Image Management"
        description="Manage diskless boot images and their snapshots"
        actions={
          <div className="flex items-center gap-2">
            <Button
              variant="ghost"
              onClick={handleImportExisting}
              icon={Download}
              disabled={importing}
              title="Scan the ZFS pool and register existing boot images and snapshots"
            >
              {importing ? "Scanning..." : "Import Existing"}
            </Button>
            {masters.length > 0 && (
              <Button
                variant="primary"
                onClick={handleCreateImage}
                icon={PlusCircle}
                disabled={!hasImageDisk}
                title={
                  !hasImageDisk
                    ? "No image disk found. Create an image disk first."
                    : "Create Image"
                }
              >
                Create Image
              </Button>
            )}
          </div>
        }
      />
      <div className="space-y-6 min-h-[50vh]">
        {masters.length === 0 ? (
          <ShadcnCard className="shadow-sm border border-border/50">
            <CardContent className="flex flex-col gap-4 items-center text-center p-12">
              <div className="w-20 h-20 bg-muted rounded-full flex items-center justify-center text-4xl mb-4">
                <File />
              </div>
              <CardTitle className="text-2xl mb-2">No Images Available</CardTitle>
              <CardDescription className="max-w-md mb-6">
                {!hasImageDisk
                  ? "No image disk found. Create an image disk first in the Disks Management section."
                  : "Create your first Boot image for clients to boot from."}
              </CardDescription>
              <Button
                variant="primary"
                onClick={handleCreateImage}
                icon={PlusCircle}
                disabled={!hasImageDisk}
              >
                Create Image
              </Button>
              {!hasImageDisk && (
                <Link to="/disks" className={buttonVariants({ variant: "link" })}>
                  Create an image disk first.
                </Link>
              )}
            </CardContent>
          </ShadcnCard>
        ) : (
          <ImagesList masters={masters} />
        )}
      </div>
      {openImageCreateModal && (
        <CreateImageModal
          openImageCreateModal={openImageCreateModal}
          setOpenImageCreateModal={setOpenImageCreateModal}
        />
      )}
    </div>
  );
};

export default ImageManagement;
