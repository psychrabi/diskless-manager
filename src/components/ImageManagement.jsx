import React from 'react';
import {
  PlusCircle, Trash2, Save, GitBranchPlus, HardDrive, Star, StarIcon,
  Plus
} from 'lucide-react';
import { useMasterManager } from '../hooks/useMasterManager';
import { Card, Button, Modal, Input, Select } from '../components/ui';
import { useNotification } from '../contexts/NotificationContext';
const API_BASE_URL = 'http://192.168.1.250:5000/api'; // !!! IMPORTANT: Replace with your backend server IP/hostname and port !!!

export const ImageManagement = ({ masters, refresh }) => {
  
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
    formatBytes,
    formatDate
  } = useMasterManager(refresh);



  return (
    <div className="space-y-6">
      <Card title="Image Management" icon={HardDrive} actions={ // Add button to card actions
        <Button onClick={handleOpenCreateMasterModal} icon={PlusCircle}>Create Image</Button>
      }>
          <div className="space-y-6">
          {masters.map((master) => (
            <div key={master.id} className="p-4 border border-gray-200 dark:border-gray-700 rounded-md bg-gray-50 dark:bg-gray-700/50">
              <div className="flex flex-wrap justify-between items-center mb-3 gap-2">
                <div className="flex items-center gap-2">
                  <h4 className="text-lg font-medium break-all flex items-center gap-1">
                    {master.name}
                    {master.is_default && <StarIcon className="h-4 w-4 text-yellow-500 fill-yellow-500" />}
                  </h4>
                </div>
                <div className="flex gap-2">
                  <Button 
                    variant={master.is_default ? 'outline' : 'ghost'} 
                    size="sm" 
                    onClick={() => setDefaultMaster(master.name)}
                    className={master.is_default ? 'text-green-500 border-green-500' : ''}
                    disabled={master.is_default} 
                  >
                    {master.is_default ? (
                      <span className="flex items-center gap-1">
                        <Star className="h-4 w-4" /> Default
                      </span>
                    ) : 'Set as Default'}
                  </Button>
                  <Button onClick={() => handleOpenCreateSnapshotModal(master.name)} size="sm" icon={PlusCircle}>Create Snapshot</Button>
                  <Button variant="destructive" onClick={() => handleOpenDeleteMasterModal(master.name)} size="sm" icon={Trash2}>Delete Master</Button>
                </div>
              </div>
              <h5 className="text-sm font-semibold mb-2 text-gray-600 dark:text-gray-400">Available Snapshots:</h5>
              {master.snapshots && master.snapshots.length > 0 ? ( // Check if snapshots exist
                <ul className="space-y-2 text-sm">
                  {master.snapshots.map((snap) => (
                    <li key={snap.id || snap.name} className="flex flex-wrap justify-between items-center gap-2 p-2 rounded hover:bg-gray-100 dark:hover:bg-gray-600/50">
                      <div className="flex-1 min-w-0">
                        <span className="font-mono text-xs break-all">{snap.name}</span>
                        <span className="text-gray-500 dark:text-gray-400 text-xs ml-2 whitespace-nowrap">({snap.created}, {snap.size})</span>
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
          {masters.length === 0 && <p className="text-center py-4 text-gray-500">No master images found.</p>}
        </div>
      </Card>
      <Modal isOpen={isCreateMasterModalOpen} onClose={() => setIsCreateMasterModalOpen(false)} title="Create Master Image" size='xl'>
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
                  This will create a ZFS volume named '{newMasterName ? `${newMasterName}-master` : '...-master'}' in the '{API_BASE_URL.includes('localhost') ? 'tank' : 'configured'}' pool.
               </p>
              <div className="mt-6 flex justify-end space-x-3">
                  <Button type="button" variant="outline" onClick={() => setIsCreateMasterModalOpen(false)}>Cancel</Button>
                  <Button type="submit" icon={Save}>Create Master</Button>
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
            <Button type="button" variant="outline" onClick={() => setIsCreateSnapshotModalOpen(false)}>Cancel</Button>
            <Button type="submit" icon={Save}>Create Snapshot</Button>
          </div>
        </form>
      </Modal>
      <Modal 
          isOpen={isDeleteSnapshotModalOpen} 
          onClose={cancelDeleteSnapshot}
          title="Delete Snapshot"
      >
          <div className="space-y-4">
              <p>
                  Are you sure you want to delete snapshot "{snapshotToDelete}"?
                  This action cannot be undone and might affect clones.
              </p>
              
              <div className="flex justify-end space-x-3">
                  <Button
                      variant="outline"
                      onClick={cancelDeleteSnapshot}
                  >
                      Cancel
                  </Button>
                  <Button
                      variant="destructive"
                      onClick={confirmDeleteSnapshot}
                  >
                      Delete Snapshot
                  </Button>
              </div>
          </div>
      </Modal>
      <Modal 
          isOpen={isDeleteMasterModalOpen} 
          onClose={cancelDeleteMaster}
          title="Delete Snapshot"
      >
          <div className="space-y-4">
              <p>
                  Are you sure you want to delete Master "{selectedMaster}"?
                  This action cannot be undone and might affect clones.
              </p>
              
              <div className="flex justify-end space-x-3">
                  <Button
                      variant="outline"
                      onClick={cancelDeleteMaster}
                  >
                      Cancel
                  </Button>
                  <Button
                      variant="destructive"
                      onClick={confirmDeleteMaster}
                  >
                      Delete Master
                  </Button>
              </div>
          </div>
      </Modal>
    </div>
  );
};

export default ImageManagement;
