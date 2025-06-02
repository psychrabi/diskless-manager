import { HardDrive, PlusCircle, Save, Star, StarIcon, Trash2 } from 'lucide-react';
import { useCallback, useMemo } from 'react';
import { useMasterManager } from '../../hooks/useMasterManager';
import { useAppStore } from '../../store/useAppStore';
import { Button, Card, Input, Modal } from '../ui';
import { useLoaderData } from 'react-router-dom';

export const ImageManagement = () => {
  const { fetchData } = useAppStore();
  const { masters } = useLoaderData();
  const {
    handleCreateSnapshot,
    handleDeleteSnapshot,
    handleOpenCreateSnapshotModal,
    isCreateSnapshotModalOpen,
    setIsCreateSnapshotModalOpen,
    selectedMaster,
    newSnapshotName,
    setNewSnapshotName,
    handleOpenCreateMasterModal,
    handleCreateMasterSubmit,
    isCreateMasterModalOpen,
    setIsCreateMasterModalOpen,
    newMasterName,
    setNewMasterName,
    newMasterSize,
    setNewMasterSize,
    isDeleteSnapshotModalOpen,
    snapshotToDelete,
    confirmDeleteSnapshot,
    cancelDeleteSnapshot,
    handleOpenDeleteMasterModal,
    cancelDeleteMaster,
    confirmDeleteMaster,
    setDefaultMaster,
    isDeleteMasterModalOpen,
  } = useMasterManager(fetchData);

  const memoizedMasters = useMemo(() => masters, [masters]);
  const memoizedSetDefaultMaster = useCallback(setDefaultMaster, [setDefaultMaster]);
  const memoizedHandleOpenCreateSnapshotModal = useCallback(handleOpenCreateSnapshotModal, [handleOpenCreateSnapshotModal]);
  const memoizedHandleOpenDeleteMasterModal = useCallback(handleOpenDeleteMasterModal, [handleOpenDeleteMasterModal]);

  return (
    <div className="space-y-6">
      <Card title="Image Management" icon={HardDrive} actions={
        <Button variant="primary" onClick={handleOpenCreateMasterModal} icon={PlusCircle}>Create Image</Button>
      }>
        <div className="space-y-6">
          {memoizedMasters.map((master) => (
            <div key={master.id} className="p-4 border border-gray-200 dark:border-gray-700 rounded-md bg-gray-50 dark:bg-gray-700/50">
              <div className="flex flex-wrap justify-between items-center mb-3 gap-2">
                <div className="flex items-center gap-2">
                  <h4 className="text-lg font-medium break-all flex items-center gap-1">
                    {master.name} {`(${master.size})`}
                    {master.is_default && <StarIcon className="h-4 w-4 text-yellow-500 fill-yellow-500" />}
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
                  <Button variant='primary' onClick={() => memoizedHandleOpenCreateSnapshotModal(master.name)} size="sm" icon={PlusCircle}>Create Snapshot</Button>
                  <Button variant="destructive" onClick={() => memoizedHandleOpenDeleteMasterModal(master.name)} size="sm" icon={Trash2}>Delete Master</Button>
                </div>
              </div>
              <h5 className="text-sm font-semibold mb-2 text-gray-600 dark:text-gray-400">Available Snapshots:</h5>
              {master.snapshots && master.snapshots.length > 0 ? (
                <ul className="space-y-2 text-sm">
                  {master.snapshots.map((snap) => (
                    <li key={snap.id || snap.name} className="flex flex-wrap justify-between items-center gap-2 p-2 rounded hover:bg-gray-100 dark:hover:bg-gray-600/50">
                      <div className="flex-1 min-w-0">
                        <span className="font-mono text-xs break-all">{snap.name}</span>
                        <span className="text-gray-500 dark:text-gray-400 text-xs ml-2 whitespace-nowrap">({snap.created}, {snap.used})</span>
                      </div>
                      <div className="flex space-x-1 flex-shrink-0">
                        <Button onClick={() => handleDeleteSnapshot(snap.name)} variant="destructive" size="icon" className="h-7 w-7 text-red-500 hover:bg-red-100 dark:hover:bg-red-900/50" title={`Delete ${snap.name}`}>
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
      <Modal isOpen={isCreateMasterModalOpen} onClose={() => setIsCreateMasterModalOpen(false)} title="Create Master Image">
        <form onSubmit={handleCreateMasterSubmit}>
          <Input
            label="Master Name:" id="masterName" value={newMasterName}
            onChange={(e) => setNewMasterName(e.target.value)}
            placeholder="e.g., win11-enterprise (will create pool/name-master)"
            required
          />
          <Input
            label="Size:" id="masterSize" value={newMasterSize}
            onChange={(e) => setNewMasterSize(e.target.value)}
            placeholder="e.g., 50G, 1T"
            title="Enter size (e.g., 50G, 100G, 1T)" required
            className='mt-4'
          />
          <p className="text-xs text-gray-500 dark:text-gray-400 mt-1 mb-4">
            This will create a ZFS volume named '{newMasterName ? `${newMasterName}-master` : '...-master'}' in the pool.
          </p>
          <div className="mt-6 flex justify-end space-x-3">
            <Button type="submit" variant="primary" icon={Save}>Create Master</Button>
            <Button type="button" variant="destructive" onClick={() => setIsCreateMasterModalOpen(false)}>Cancel</Button>
          </div>
        </form>
      </Modal>
      {/* Create Snapshot Modal */}
      <Modal isOpen={isCreateSnapshotModalOpen} onClose={() => setIsCreateSnapshotModalOpen(false)} title={`Create Snapshot for ${selectedMaster}`} size='3xl'>
        <form onSubmit={(e) => {
          e.preventDefault();
          // Construct the full snapshot name: selectedMaster.name + '@' + newSnapshotName
          const fullSnapshotName = `${selectedMaster}@${newSnapshotName}`;
          handleCreateSnapshot(fullSnapshotName); // Pass the constructed full name
        }}>
          <Input
            label="Snapshot Name (e.g., base-install, pre-updates):"
            id="snapshotName"
            value={newSnapshotName}
            onChange={(e) => setNewSnapshotName(e.target.value)} // Only the snapshot name, not the full path
            placeholder="Enter snapshot name (e.g., my-snapshot-name)"
            required
          />
          <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
            This operation will capture the current state of <strong className="font-semibold">{selectedMaster}</strong>.
            This operation cannot be undone.
          </p>
          <div className="mt-6 flex justify-end space-x-3">
            <Button type="submit" variant="primary" icon={Save}>Create Snapshot</Button>
            <Button type="button" variant="destructive" onClick={() => setIsCreateSnapshotModalOpen(false)}>Cancel</Button>
          </div>
        </form>
      </Modal>
      <Modal isOpen={isDeleteSnapshotModalOpen} onClose={cancelDeleteSnapshot} title="Delete Snapshot">
        <div className="space-y-4">
          <p>
            Are you sure you want to delete snapshot "{snapshotToDelete}"?
            This action cannot be undone and might affect clones.
          </p>
          <div className="flex justify-end space-x-3">
            <Button variant="primary" onClick={confirmDeleteSnapshot}>
              Delete Snapshot
            </Button>
            <Button variant="destructive" onClick={cancelDeleteSnapshot} >
              Cancel
            </Button>
          </div>
        </div>
      </Modal>
      <Modal isOpen={isDeleteMasterModalOpen} onClose={cancelDeleteMaster} title="Delete Snapshot" >
        <div className="space-y-4">
          <p>
            Are you sure you want to delete Master "{selectedMaster}"?
            This action cannot be undone and might affect clones.
          </p>
          <div className="flex justify-end space-x-3">
            <Button variant="primary" onClick={confirmDeleteMaster} >
              Delete Master
            </Button>
            <Button variant="destructive" onClick={cancelDeleteMaster} >
              Cancel
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};

export default ImageManagement;
