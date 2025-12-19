import { HardDrive, PlusCircle } from "lucide-react";
import { useState } from "react";
import { useAppStore } from "../../store/useAppStore";
import { Button, Card } from "../ui";
import CreateImageModal from "./CreateImageModal";
import { ImagesList } from "./ImagesList";

const ImageManagement = () => {
  const masters = useAppStore((state) => state.masters);
  const [openImageCreateModal, setOpenImageCreateModal] = useState(false);

  const handleCreateImage = () => setOpenImageCreateModal(true);

  return (
    <Card
      title="Image Management"
      className="bg-base-300"
      icon={HardDrive}
      actions={
        <Button
          variant="primary"
          onClick={() => handleCreateImage()}
          icon={PlusCircle}
        >
          Create Image
        </Button>
      }
    >
      <div className="space-y-6 min-h-[calc(100vh-14rem)]">
        <ImagesList masters={masters} />
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
