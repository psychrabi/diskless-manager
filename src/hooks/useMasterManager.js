import { useState, useCallback } from 'react';
import { formatBytes, formatDate } from '../utils/helpers';
import { apiRequest, handleApiAction } from '../utils/apiRequest';
import { useNotification } from '../contexts/NotificationContext';
import { useNotification } from '../contexts/NotificationContext';

export const useMasterManager = (refresh) => {
export const useMasterManager = (refresh) => {
  const [isCreateSnapshotModalOpen, setIsCreateSnapshotModalOpen] = useState(false);
  const [selectedMaster, setSelectedMaster] = useState(null);
  const [newSnapshotName, setNewSnapshotName] = useState('');
  const [isCreateMasterModalOpen, setIsCreateMasterModalOpen] = useState(false);
  const [newMasterName, setNewMasterName] = useState('');
  const [newMasterSize, setNewMasterSize] = useState('50G');
  const [isDeleteSnapshotModalOpen, setIsDeleteSnapshotModalOpen] = useState(false);
  const [snapshotToDelete, setSnapshotToDelete] = useState(null);
  const [isDeleteMasterModalOpen, setIsDeleteMasterModalOpen] = useState(false);
  const {showNotification} = useNotification();  

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

  



  const setDefaultMaster = async (masterName) => {    
     await handleApiAction(
        () => apiRequest('/masters/default', 'POST', { name: masterName }),
        `ZVOL ${masterName} has been set as default.`,
        `Failed to set ZVOL ${masterName} as default`,
        showNotification
    )
  };

  const confirmDeleteMaster = () => {
    if (!selectedMaster) return;
    
    const encodedMasterName = encodeURIComponent(selectedMaster);
    handleApiAction(
        () => apiRequest(`/masters/${encodedMasterName}`, 'DELETE'),
        `Master ${selectedMaster} deleted successfully.`,
        `Failed to delete master ${selectedMaster}`,
        showNotification
    ).then(() => {
        refresh(); // Refresh data after creating master
    });
    setIsDeleteMasterModalOpen(false);
    setSelectedMaster(null);
  };

  const handleOpenDeleteMasterModal = useCallback((master) => {
    setSelectedMaster(master);    
    setIsDeleteMasterModalOpen(true);
  }, []);

  const cancelDeleteMaster = () => {
    setIsDeleteMasterModalOpen(false);
    setSelectedMaster(null);
  };

  const cancelDeleteSnapshot = () => {
    setIsDeleteSnapshotModalOpen(false);
    setSnapshotToDelete(null);
  };    
  };    

  const handleOpenCreateSnapshotModal = useCallback((master) => {
    setSelectedMaster(master);    
    setIsCreateSnapshotModalOpen(true);
  }, []);

 
 

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
    cancelDeleteSnapshot,
    cancelDeleteMaster,
    setDefaultMaster,
    handleOpenDeleteMasterModal,
    isDeleteMasterModalOpen,
    confirmDeleteMaster
  };
};
