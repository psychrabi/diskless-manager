import { File, HardDrive, PlusCircle } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router-dom";
import { useZfs } from "../../hooks/useZfs";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card } from "../ui";
import CreateImageModal from "./CreateImageModal";
import { ImagesList } from "./ImagesList";

const ImageManagement = () => {
  const masters = useAppStore((state) => state.masters);
  const [openImageCreateModal, setOpenImageCreateModal] = useState(false);
  const { datasets } = useZfs();

  // Check if there are any image disks (datasets with org.diskless:type=image)
  const hasImageDisk = datasets.some(dataset =>
    dataset.disk_type && dataset.disk_type.toLowerCase() === 'image'
  );

  const handleCreateImage = () => {
    if (hasImageDisk) {
      setOpenImageCreateModal(true);
    }
  };

  return (
    <Card
      title="Image Management"
      className="bg-base-300"
      icon={HardDrive}
      actions={masters.length > 0 &&
        <Button
          variant="primary"
          onClick={handleCreateImage}
          icon={PlusCircle}
          disabled={!hasImageDisk}
          title={!hasImageDisk ? "No image disk found. Create an image disk first." : "Create Image"}
        >
          Create Image
        </Button>
      }
    >
      <div className="space-y-6 min-h-[calc(100vh-14rem)]">
        {masters.length === 0 ? (
          <div className="card bg-base-100 shadow-xl border border-base-200/50">
            <div className="card-body items-center text-center p-12">
              <div className="w-20 h-20 bg-base-200 rounded-full flex items-center justify-center text-4xl mb-4">
                <File />
              </div>
              <h2 className="card-title text-2xl mb-2">No Images Available</h2>
              <p className="text-base-content/60 max-w-md mb-6">
                {!hasImageDisk
                  ? "No image disk found. Create an image disk first in the Disks Management section."
                  : "Create your first Boot image for clients to boot from."
                }
              </p>
              <Button
                variant="primary"
                onClick={handleCreateImage}
                icon={PlusCircle}
                disabled={!hasImageDisk}
              >
                Create Image
              </Button>
              {!hasImageDisk && (
                <Link to="/disks" className="btn btn-link">
                  Create an image disk first.
                </Link>
              )}

            </div>
          </div>
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
    </Card>
  );
};

export default ImageManagement;
