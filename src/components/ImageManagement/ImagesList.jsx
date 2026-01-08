import { useMasterManager } from "@/hooks/useMasterManager";
import { Edit, PlusCircle, Star, StarIcon, Trash2 } from "lucide-react";
import { useCallback, useState } from "react";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import CreateSnapshotModal from "./CreateSnapshotModal";
import RenameImageModal from "./RenameImageModal";
import { SnapshotsList } from "./SnapshotsList";

const ImageCard = ({
  master,
  handleCreateSnapshot,
  handleRenameImage,
  handleDeleteImage,
  memoizedSetDefaultMaster,
}) => {

  const actions = (
    <div className="join join-horizontal">
      <Button
        className="join-item"
        variant={master.is_default ? "success" : "default"}
        size="icon"
        icon={master.is_default ? Star : StarIcon}
        onClick={() => memoizedSetDefaultMaster(master.name)}
        disabled={master.is_default}
        title={!master.is_default ? "Set as Default" : "Remove from Default"}
      />


      <Button
        variant="primary"
        className="join-item"
        onClick={() => handleCreateSnapshot(master.name)}
        size="icon"
        icon={PlusCircle}
        title={"Create Snapshot"}
      />
      <Button
        variant="info"
        className="join-item"
        onClick={() => handleRenameImage(master)}
        size="icon"
        icon={Edit}
        title="Rename Image"
      />
      <Button
        variant="destructive"
        className="join-item"
        onClick={() => handleDeleteImage(master)}
        size="icon"
        icon={Trash2}
        title="Delete Image"
      />
    </div>
  )

  return <Card title={`${master.name} (${master.size_gb}GB)`} subtitle={master.path} actions={actions}>
    <h5 className="text-sm font-semibold mb-2 text-base-content/70">
      Available Snapshots:
    </h5>
    {master.snapshots && master.snapshots.length > 0 ? (
      <SnapshotsList master={master} />
    ) : (
      <p className="text-sm text-base-content/60">
        No snapshots found for this master.
      </p>
    )}
  </Card>
};

export const ImagesList = ({ masters }) => {
  const { setDefaultMaster, handleDeleteImage } = useMasterManager();
  const memoizedSetDefaultMaster = useCallback(
    (masterName) => setDefaultMaster(masterName),
    [setDefaultMaster]
  );
  const [openSnapshotCreateModal, setOpenSnapshotCreateModal] = useState(false);
  const [selectedImage, setSelectedImage] = useState("");
  const [openRenameModal, setOpenRenameModal] = useState(false);

  const handleCreateSnapshot = useCallback((image) => {
    setSelectedImage(image);
    setOpenSnapshotCreateModal(true);
  }, []);

  const handleRenameImage = useCallback((image) => {
    setSelectedImage(image);
    setOpenRenameModal(true);
  }, []);

  return (
    <>
      {masters.map((master) => (
        <ImageCard
          key={master.id}
          master={master}
          handleCreateSnapshot={handleCreateSnapshot}
          handleRenameImage={handleRenameImage}
          handleDeleteImage={handleDeleteImage}
          memoizedSetDefaultMaster={memoizedSetDefaultMaster}
        />
      ))}
      {openSnapshotCreateModal && (
        <CreateSnapshotModal
          openSnapshotCreateModal={openSnapshotCreateModal}
          setOpenSnapshotCreateModal={setOpenSnapshotCreateModal}
          selectedImage={selectedImage}
        />
      )}
      {openRenameModal && (
        <RenameImageModal
          openRenameModal={openRenameModal}
          setOpenRenameModal={setOpenRenameModal}
          selectedImage={selectedImage}
        />
      )}
    </>
  );
};
