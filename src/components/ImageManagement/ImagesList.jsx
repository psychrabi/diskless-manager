import { useMasterManager } from "@/hooks/useMasterManager";
import { Edit, PlusCircle, Star, StarIcon, Trash2 } from 'lucide-react';
import React, { useCallback, useState } from "react";
import CreateSnapshotModal from "./CreateSnapshotModal";
import RenameImageModal from "./RenameImageModal";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { SnapshotsList } from "./SnapshotsList";

const ImageCard = React.memo(({ master, handleCreateSnapshot, handleRenameImage, handleDeleteImage, memoizedSetDefaultMaster }) => (
  <Card className="rounded-lg pt-2">
    <div className="flex flex-wrap justify-between items-center mb-3 gap-2">
      <div className="flex items-center gap-2">
        <h4 className="text-lg font-medium break-all flex items-center gap-1">
          {master.name} {`(${master.size})`}
          {master.is_default && <StarIcon className="h-4 w-4 text-warning fill-warning" />}
        </h4>
      </div>
      <div className="flex gap-2 ">
        <Button variant={master.is_default ? 'accent' : 'success'} size="sm" onClick={() => memoizedSetDefaultMaster(master.name)} disabled={master.is_default} >
          {master.is_default ? (<span className="flex items-center gap-1"><Star className="h-4 w-4" /> Default</span>) : 'Set as Default'}
        </Button>
        <Button variant='primary' onClick={() => handleCreateSnapshot(master.name)} size="sm" icon={PlusCircle} title={'Create Snapshot'} >
          Create Snapshot
        </Button>
        <Button variant="info" onClick={() => handleRenameImage(master.name)} size="sm" icon={Edit}>Rename</Button>
        <Button variant="destructive" onClick={() => handleDeleteImage(master.name)} size="sm" icon={Trash2}>Delete Image</Button>
      </div>
    </div>
    <h5 className="text-sm font-semibold mb-2 text-base-content/70">Available Snapshots:</h5>
    {master.snapshots && master.snapshots.length > 0 ? <SnapshotsList master={master} /> : <p className="text-sm text-base-content/60">No snapshots found for this master.</p>}
  </Card>
));

export const ImagesList = React.memo(({ masters }) => {
  const { setDefaultMaster, handleDeleteImage } = useMasterManager();
  const memoizedSetDefaultMaster = useCallback((masterName) => setDefaultMaster(masterName), [setDefaultMaster]);
  const [openSnapshotCreateModal, setOpenSnapshotCreateModal] = useState(false)
  const [selectedImage, setSelectedImage] = useState('')
  const [openRenameModal, setOpenRenameModal] = useState(false)

  const handleCreateSnapshot = useCallback((image) => {
    setSelectedImage(image)
    setOpenSnapshotCreateModal(true)
  }, []);

  const handleRenameImage = useCallback((image) => {
    setSelectedImage(image)
    setOpenRenameModal(true)
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
      {openSnapshotCreateModal && <CreateSnapshotModal openSnapshotCreateModal={openSnapshotCreateModal} setOpenSnapshotCreateModal={setOpenSnapshotCreateModal} selectedImage={selectedImage} />}
      {openRenameModal && <RenameImageModal openRenameModal={openRenameModal} setOpenRenameModal={setOpenRenameModal} selectedImage={selectedImage} />}
    </>
  )
});