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
  const [isDeleteSnapshotModalOpen, setIsDeleteSnapshotModalOpen] = useState(false);
  const [snapshotToDelete, setSnapshotToDelete] = useState(null);

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
    refresh();
};

const handleCreateSnapshot = (snapshotName) => {  
      handleApiAction(
          () => apiRequest('/snapshots', 'POST', { name: snapshotName }),
          `Snapshot ${snapshotName} created successfully.`,
          `Failed to create snapshot ${snapshotName}`,
          showNotification  
      );
      setIsCreateSnapshotModalOpen(false);
      refresh();
};



  const handleDeleteSnapshot = (snapshotName) => {
    setSnapshotToDelete(snapshotName);
    setIsDeleteSnapshotModalOpen(true);
  };

  const confirmDeleteSnapshot = () => {
    if (!snapshotToDelete) return;
    
    const encodedSnapshotName = encodeURIComponent(snapshotToDelete);
    handleApiAction(
        () => apiRequest(`/snapshots/${encodedSnapshotName}`, 'DELETE'),
        `Snapshot ${snapshotToDelete} deleted successfully.`,
        `Failed to delete snapshot ${snapshotToDelete}`,
        showNotification
    );
    setIsDeleteSnapshotModalOpen(false);
    setSnapshotToDelete(null);
  };

  const cancelDeleteSnapshot = () => {
    setIsDeleteSnapshotModalOpen(false);
    setSnapshotToDelete(null);
  };

  const handleOpenCreateSnapshotModal = useCallback((master) => {
    setSelectedMaster(master);    
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
    refresh();
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
    formatDate,
    isDeleteSnapshotModalOpen,
    snapshotToDelete,
    confirmDeleteSnapshot,
    cancelDeleteSnapshot
  };
};
