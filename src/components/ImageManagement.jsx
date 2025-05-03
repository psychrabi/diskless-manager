import React from 'react';
import {
  PlusCircle, Trash2, Save, GitBranchPlus
} from 'lucide-react';
import { useMasterManager } from '../hooks/useMasterManager';
import { Card, Button, Modal, Input, Select } from '../components/ui';

export const ImageManagement = ({ masters, refresh }) => {
  const {
    handleCreateSnapshot,
    handleDeleteSnapshot,
    handleCloneSnapshot,
    handleOpenCreateSnapshotModal,
    isCreateSnapshotModalOpen,
    setIsCreateSnapshotModalOpen,
    selectedMaster,
    newSnapshotName,
    setNewSnapshotName,
    formatBytes,
    formatDate
  } = useMasterManager(masters, refresh);

  return (
    <Card title="Image Management" icon={PlusCircle}>
      <div className="space-y-6">
        {masters.map(master => (
          <div key={master.id} className="p-4 border border-gray-200 dark:border-gray-700 rounded-md bg-gray-50 dark:bg-gray-700/50">
            <div className="flex flex-wrap justify-between items-center mb-3 gap-2">
              <h4 className="text-lg font-medium break-all">{master.name}</h4>
              <Button onClick={() => handleOpenCreateSnapshotModal(master)} size="sm" icon={PlusCircle}>Create Snapshot</Button>
            </div>
            <h5 className="text-sm font-semibold mb-2 text-gray-600 dark:text-gray-400">Available Snapshots:</h5>
            {master.snapshots.length > 0 ? (
              <ul className="space-y-2">
                {master.snapshots.map(snap => (
                  <li key={snap.id} className="flex items-center justify-between p-2 rounded-md hover:bg-gray-100 dark:hover:bg-gray-800/50">
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="font-medium break-all">{snap.name}</span>
                        <span className="text-sm text-gray-500 dark:text-gray-400">• {formatDate(snap.created)}</span>
                      </div>
                      <div className="text-sm text-gray-500 dark:text-gray-400">{formatBytes(snap.size)}</div>
                    </div>
                    <div className="flex space-x-1 flex-shrink-0">
                      <Button onClick={() => handleCloneSnapshot(snap.name)} variant="outline" size="icon" className="h-7 w-7" title={`Clone ${snap.name} to new master`}>
                        <GitBranchPlus className="h-4 w-4" />
                      </Button>
                      <Button onClick={() => handleDeleteSnapshot(snap.name)} variant="ghost" size="icon" className="h-7 w-7 text-red-500 hover:bg-red-100 dark:hover:bg-red-900/50" title={`Delete ${snap.name}`}>
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="text-gray-500 dark:text-gray-400">No snapshots available</p>
            )}
          </div>
        ))}
      </div>

      {/* Create Snapshot Modal */}
      <Modal isOpen={isCreateSnapshotModalOpen} onClose={() => setIsCreateSnapshotModalOpen(false)} title="Create Snapshot">
        <form onSubmit={(e) => {
          e.preventDefault();
          handleCreateSnapshot(newSnapshotName);
        }}>
          <Input
            label="Snapshot Name:"
            id="snapshotName"
            value={newSnapshotName}
            onChange={(e) => setNewSnapshotName(e.target.value)}
            placeholder="e.g., tank/win10-master@2025-05-01"
            required
          />
          <p className="mt-2 text-sm text-gray-500 dark:text-gray-400">
            Creating a snapshot will capture the current state of {selectedMaster?.name}.
            This operation cannot be undone.
          </p>
          <div className="mt-6 flex justify-end space-x-3">
            <Button type="button" variant="outline" onClick={() => setIsCreateSnapshotModalOpen(false)}>Cancel</Button>
            <Button type="submit" icon={Save}>Create Snapshot</Button>
          </div>
        </form>
      </Modal>
    </Card>
  );
};

export default ImageManagement;
