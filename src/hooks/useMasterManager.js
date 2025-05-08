import { useState, useCallback } from 'react';
import { formatBytes, formatDate } from '../utils/helpers';
import { apiRequest, handleApiAction } from '../utils/apiRequest';

export const useMasterManager = (masters, refresh, showNotification) => {
  const [isCreateSnapshotModalOpen, setIsCreateSnapshotModalOpen] = useState(false);
  const [selectedMaster, setSelectedMaster] = useState(null);
  const [newSnapshotName, setNewSnapshotName] = useState('');
  const [isCreateMasterModalOpen, setIsCreateMasterModalOpen] = useState(false);
  const [newMasterName, setNewMasterName] = useState('');
  const [newMasterSize, setNewMasterSize] = useState('50G');

// --- Master/Snapshot Actions ---
  const handleOpenCreateMasterModal = () => {
    setNewMasterName('');
    setNewMasterSize('50G'); // Reset to default
    setIsCreateMasterModalOpen(true);
};

const handleCreateMasterSubmit = async (event) => {
    event.preventDefault();
    setIsCreateMasterModalOpen(false); // Close modal
    await handleApiAction(
        () => apiRequest('/masters', 'POST', { name: newMasterName, size: newMasterSize }),
        `Master ZVOL ${newMasterName}-master created successfully.`,
        `Failed to create master ZVOL ${newMasterName}-master`,
        showNotification
    );
};

const handleCreateSnapshot = (snapshotName) => {  
      handleApiAction(
          () => apiRequest('/snapshots', 'POST', { name: snapshotName }),
          `Snapshot ${snapshotName} created successfully.`,
          `Failed to create snapshot ${snapshotName}`,
          showNotification  
      );
};



const handleDeleteSnapshot = (snapshotName) => {
   const encodedSnapshotName = encodeURIComponent(snapshotName);
   if (confirm(`Are you sure you want to delete snapshot "${snapshotName}"? This cannot be undone and might affect clones.`)) {
       handleApiAction(
          () => apiRequest(`/snapshots/${encodedSnapshotName}`, 'DELETE'),
          `Snapshot ${snapshotName} deleted successfully.`,
          `Failed to delete snapshot ${snapshotName}`,
          showNotification
      );
  }
};



  const handleOpenCreateSnapshotModal = useCallback((master) => {
    setSelectedMaster(master);
    setNewSnapshotName(`${master}@${new Date().toISOString().slice(0,10)}`);
    setIsCreateSnapshotModalOpen(true);
  }, []);

 

  const handleCloneSnapshot = (snapshotName) => {
    const baseMaster = snapshotName.split('@')[0].split('/')[1];
    const defaultCloneName = `tank/${baseMaster}-clone-${Date.now().toString().slice(-4)}`;
    const newMasterName = prompt(`Enter name for the new master ZVOL to be cloned from ${snapshotName}:`, defaultCloneName);
     if (newMasterName) {
        handleApiAction(
          () => apiRequest('/masters', 'POST', { name: newMasterName, size: newMasterSize }),
          `Master ZVOL ${newMasterName}-master created successfully.`,
          `Failed to create master ZVOL ${newMasterName}-master`,
          showNotification
      );
    }
  };

  

  return {
    isCreateSnapshotModalOpen,
    isCreateMasterModalOpen,
    selectedMaster,
    newSnapshotName,
    setIsCreateSnapshotModalOpen,

    setIsCreateMasterModalOpen,
    setNewSnapshotName,
    handleCreateSnapshot,
    handleDeleteSnapshot,
    handleCloneSnapshot,
    handleOpenCreateSnapshotModal,
    handleCreateMasterSubmit,
    handleOpenCreateMasterModal,
    newMasterName,
    newMasterSize,
    setNewMasterName,
    setNewMasterSize,
    formatBytes,
    formatDate
  };
};
