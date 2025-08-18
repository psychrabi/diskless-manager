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
  const { showNotification } = useNotification();
  const { setDefaultMaster } = useMasterManager();
  const [openImageCreateModal, setOpenImageCreateModal] = useState(false)
  const [openSnapshotCreateModal, setOpenSnapshotCreateModal] = useState(false)
  const [selectedImage, setSelectedImage] = useState('')  
  const [openRenameModal, setOpenRenameModal] = useState(false)
  const confirm = useConfirm();

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

  const handleRollbackSnapshot = async (snapshot, image) => {
    if (!snapshot || !image) return;
    const ok = await confirm({
      title: 'Rollback Snapshot',
      description: `Are you sure you want to rollback snapshot "${snapshot}" for image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: 'Rollback Snapshot',
      cancelText: 'Cancel',
      confirmVariant: 'primary',
      size: '2xl',
    });
    if (ok) {
      const token = localStorage.getItem('authToken') || '';
      await invoke('rollback_master_snapshot', { token, masterName: image, snapshotName: snapshot })
        .then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => {
          showNotification(error, 'error',)
        })
    } else {
      showNotification("Snapshot rollback cancelled", 'error',)
    }
  }

  const handleDeleteImage = async (image) => {
    if (!image) return;
    const ok = await confirm({
      title: 'Delete Image',
      description: `Are you sure you want to delete image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: 'Delete Image',
      cancelText: 'Cancel',
      confirmVariant: 'primary',
      size: '2xl',
    });
    if (ok) {
      const token = localStorage.getItem('authToken') || '';
      await invoke('delete_master', { token, masterName: selectedImage })
        .then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => {
          showNotification(error, 'error',)
        })
    } else {
      showNotification("Image deletion cancelled", 'error',)
    }
  }

  const handleDeleteSnapshot = async (snapshot, image) => {
    if (!snapshot || !image) return;
    const ok = await confirm({
      title: 'Delete Snapshot',
      description: `Are you sure you want to delete snapshot "${snapshot}" for image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: 'Delete Snapshot',
      cancelText: 'Cancel',
      confirmVariant: 'primary',
      size: '2xl',
    });
    if (ok) {
      const token = localStorage.getItem('authToken') || '';
      await invoke('delete_snapshot', { token, masterName: image, snapshotName: snapshot })
        .then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => {
          showNotification(error, 'error',)
        })
    } else {
      showNotification("Snapshot deletion cancelled", 'error',)
    }
  }

  return (
    <div className="space-y-6">
      <Card title="Image Management" icon={HardDrive} actions={
        <Button variant="primary" onClick={() => handleCreateImage()} icon={PlusCircle}>Create Image</Button>
      }>
        <div className="space-y-6">
          {memoizedMasters.map((master) => (
            <div key={master.id} className="p-4 border border-gray-200 dark:border-gray-700 rounded-md bg-gray-50 dark:bg-gray-700/50">
              <div className="flex flex-wrap justify-between items-center mb-3 gap-2">
                <div className="flex items-center gap-2">
                  <h4 className="text-lg font-medium break-all flex items-center gap-1">
                    {master.name} {`(${master.size})`}
                    {master.is_default && <StarIcon className="h-4 w-4 text-yellow-500 fill-yellow-500" />}
                    {master.name.includes('/var/lib/diskless/fileio/') && (
                      <span className="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-blue-100 text-blue-800 dark:bg-blue-900 dark:text-blue-200">
                        FileIO
                      </span>
                    )}
                  </h4>
                </div>
                <div className="flex gap-2">
                  <Button variant={master.is_default ? 'accent' : 'success'} size="sm" onClick={() => memoizedSetDefaultMaster(master.name)}
                    className={master.is_default ? 'text-green-500 border-green-500' : ''} disabled={master.is_default} >
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
                    disabled={master.name.includes('/var/lib/diskless/fileio/')}
                    title={master.name.includes('/var/lib/diskless/fileio/') ? 'Snapshots not supported for FileIO images' : 'Create Snapshot'}
                  >
                    Create Snapshot
                  </Button>
                  <Button variant="info" onClick={() => handleRenameImage(master.name)} size="sm" icon={Edit}>Rename</Button>
                  <Button variant="destructive" onClick={() => handleDeleteImage(master.name)} size="sm" icon={Trash2}>Delete Image</Button>
                </div>
              </div>
              <h5 className="text-sm font-semibold mb-2 text-gray-600 dark:text-gray-400">Available Snapshots:</h5>
              {master.name.includes('/var/lib/diskless/fileio/') ? (
                <p className="text-sm text-gray-500 dark:text-gray-400">Snapshots are not supported for FileIO images.</p>
              ) : master.snapshots && master.snapshots.length > 0 ? (
                <ul className="space-y-2 text-sm">
                  {master.snapshots.map((snap) => (
                    <li key={snap.id || snap.name} className="flex flex-wrap justify-between items-center gap-2 p-2 rounded hover:bg-gray-100 dark:hover:bg-gray-600/50">
                      <div className="flex-1 min-w-0">
                        <span className="font-mono text-xs break-all">{snap.name}</span>
                        <span className="text-gray-500 dark:text-gray-400 text-xs ml-2 whitespace-nowrap">({snap.created}, {snap.used})</span>
                      </div>
                      <div className="flex space-x-1 flex-shrink-0">
                        <Button onClick={() => handleRollbackSnapshot(snap.name, master.name)} variant="info" size="icon" className="h-7 w-7 text-blue-500 hover:bg-blue-100 dark:hover:bg-blue-900/50" title={`Rollback ${snap.name}`}>
                          <RotateCcw className="h-4 w-4" />
                        </Button>
                        <Button onClick={() => handleDeleteSnapshot(snap.name, master.name)} variant="destructive" size="icon" className="h-7 w-7 text-red-500 hover:bg-red-100 dark:hover:bg-red-900/50" title={`Delete ${snap.name}`}>
                          <Trash2 className="h-4 w-4" />
                        </Button>
                      </div>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="text-sm text-gray-500 dark:text-gray-400">No snapshots found for this master.</p>
              )}
            </div>
          ))}
          {memoizedMasters.length === 0 && <p className="text-center py-4 text-gray-500">No master images found.</p>}
        </div>
      </Card>
      {openImageCreateModal && <CreateImageModal openImageCreateModal={openImageCreateModal} setOpenImageCreateModal={setOpenImageCreateModal} />}
      {openSnapshotCreateModal && <CreateSnapshotModal openSnapshotCreateModal={openSnapshotCreateModal} setOpenSnapshotCreateModal={setOpenSnapshotCreateModal} refresh={fetchData} selectedImage={selectedImage} />}
      {openRenameModal && <RenameImageModal openRenameModal={openRenameModal} setOpenRenameModal={setOpenRenameModal} selectedImage={selectedImage} refresh={fetchData} />}
    </div>
  );
};

export default ImageManagement;
