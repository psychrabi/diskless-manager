import { useConfirm } from '@/contexts/ConfirmDialogContext';
import { useNotification } from '@/contexts/NotificationContext';
import { invoke } from '@tauri-apps/api/core';
import { Edit, HardDrive, PlusCircle, RotateCcw, Star, StarIcon, Trash2 } from 'lucide-react';
import { lazy, useCallback, useMemo, useState } from 'react';
import { useMasterManager } from '../../hooks/useMasterManager';
import { useAppStore } from '../../store/useAppStore';
import { Button, Card } from '../ui';

const CreateImageModal = lazy(() => import('./CreateImageModal'));
const CreateSnapshotModal = lazy(() => import('./CreateSnapshotModal'));

const RenameImageModal = lazy(() => import('./RenameImageModal'));

const ImageManagement = () => {
  const { masters, fetchData } = useAppStore();
  const { setDefaultMaster, handleDeleteImage, handleDeleteSnapshot, handleRollbackSnapshot } = useMasterManager();
  const [openImageCreateModal, setOpenImageCreateModal] = useState(false)
  const [openSnapshotCreateModal, setOpenSnapshotCreateModal] = useState(false)
  const [selectedImage, setSelectedImage] = useState('')  
  const [openRenameModal, setOpenRenameModal] = useState(false)

  const memoizedMasters = useMemo(() => masters, [masters]);
  const memoizedSetDefaultMaster = useCallback(setDefaultMaster, [setDefaultMaster]);

  const handleCreateImage = () => setOpenImageCreateModal(true)  

  const handleCreateSnapshot = (image) => {
    setSelectedImage(image)
    setOpenSnapshotCreateModal(true)
  }

  const handleRenameImage = (image) => {
    setSelectedImage(image)
    setOpenRenameModal(true)
  }

  return (
    <div className="space-y-6">
      <Card title="Image Management" icon={HardDrive} actions={
        <Button variant="primary" onClick={() => handleCreateImage()} icon={PlusCircle}>Create Image</Button>
      }>
        <div className="space-y-6">
          {memoizedMasters.map((master) => (
            <Card key={master.id} className="p-4 rounded-md bg-base-300 shadow-xl">
              <div className="flex flex-wrap justify-between items-center mb-3 gap-2 ">
                <div className="flex items-center gap-2">
                  <h4 className="text-lg font-medium break-all flex items-center gap-1">
                    {master.name} {`(${master.size})`}
                    {master.is_default && <StarIcon className="h-4 w-4 text-warning fill-warning" />}
                  </h4>
                </div>
                <div className="flex gap-2 ">
                  <Button variant={master.is_default ? 'accent' : 'success'} size="sm" onClick={() => memoizedSetDefaultMaster(master.name)}
                    disabled={master.is_default} >
                    {master.is_default ? (
                      <span className="flex items-center gap-1">
                        <Star className="h-4 w-4" /> Default
                      </span>
                    ) : 'Set as Default'}
                  </Button>
                  <Button
                    variant='primary'
                    onClick={() => handleCreateSnapshot(master.name)}
                    size="sm"
                    icon={PlusCircle}
                    title={'Create Snapshot'}
                  >
                    Create Snapshot
                  </Button>
                  <Button variant="info" onClick={() => handleRenameImage(master.name)} size="sm" icon={Edit}>Rename</Button>
                  <Button variant="destructive" onClick={() => handleDeleteImage(master.name)} size="sm" icon={Trash2}>Delete Image</Button>
                </div>
              </div>
              <h5 className="text-sm font-semibold mb-2 text-base-content/70">Available Snapshots:</h5>
              {master.snapshots && master.snapshots.length > 0 ? (
                <ul className="space-y-2 text-sm">
                  {master.snapshots.map((snap) => (
                    <li key={snap.id || snap.name} className="flex flex-wrap justify-between items-center gap-2 p-2 rounded hover:bg-base-200">
                      <div className="flex-1 min-w-0">
                        <span className="font-mono text-xs break-all">{snap.name}</span>
                        <span className="text-base-content/60 text-xs ml-2 whitespace-nowrap">({snap.created}, {snap.used})</span>
                      </div>
                      <div className="flex space-x-1 flex-shrink-0">
                        <Button onClick={() => handleRollbackSnapshot(snap.name, master.name)} variant="info" size="icon" className="h-7 w-7" title={`Rollback ${snap.name}`}>
                          <RotateCcw className="h-4 w-4" />
                        </Button>
                        <Button onClick={() => handleDeleteSnapshot(snap.name, master.name)} variant="destructive" size="icon" className="h-7 w-7" title={`Delete ${snap.name}`}>
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-sm text-base-content/60">No snapshots found for this master.</p>
              )}
            </Card>
          ))}
          {memoizedMasters.length === 0 && <p className="text-center py-4 text-base-content/60">No master images found.</p>}
        </div>
      </Card>
      {openImageCreateModal && <CreateImageModal openImageCreateModal={openImageCreateModal} setOpenImageCreateModal={setOpenImageCreateModal} />}
      {openSnapshotCreateModal && <CreateSnapshotModal openSnapshotCreateModal={openSnapshotCreateModal} setOpenSnapshotCreateModal={setOpenSnapshotCreateModal} refresh={fetchData} selectedImage={selectedImage} />}
      {openRenameModal && <RenameImageModal openRenameModal={openRenameModal} setOpenRenameModal={setOpenRenameModal} selectedImage={selectedImage} refresh={fetchData} />}
    </div>
  );
};

export default ImageManagement;
