import { invoke } from '@tauri-apps/api/core';
import { useCallback, useState } from 'react';
import { useNotification } from '../contexts/NotificationContext';
import { formatBytes, formatDate } from '../utils/helpers';

export const useMasterManager = () => {
  const [isCreateSnapshotModalOpen, setIsCreateSnapshotModalOpen] = useState(false);
  const [selectedMaster, setSelectedMaster] = useState(null);
  const [newSnapshotName, setNewSnapshotName] = useState('');
  const [isCreateMasterModalOpen, setIsCreateMasterModalOpen] = useState(false);
  const [newMasterName, setNewMasterName] = useState('');
  const [newMasterSize, setNewMasterSize] = useState('50G');
  const [isDeleteSnapshotModalOpen, setIsDeleteSnapshotModalOpen] = useState(false);
  const [snapshotToDelete, setSnapshotToDelete] = useState(null);
  const [isDeleteMasterModalOpen, setIsDeleteMasterModalOpen] = useState(false);
  const { showNotification } = useNotification();

  // --- Master/Snapshot Actions ---
  const handleOpenCreateMasterModal = () => {
    setNewMasterName('');
    setNewMasterSize('50G'); // Reset to default
    setIsCreateMasterModalOpen(true);
  };

  const handleCreateMasterSubmit = async (event) => {
    event.preventDefault();
    setIsCreateMasterModalOpen(false); // Close modal
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    await invoke('create_master', { token, name: newMasterName, size: newMasterSize })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      }).finally(() => {
        window.location.reload();
      });

  };

  const handleCreateSnapshot = (snapshotName) => {
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    invoke('create_snapshot', { token, masterName: selectedMaster, snapshotName })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      }).finally(() => {
        window.location.reload();
      });
    setIsCreateSnapshotModalOpen(false);
  };

  const handleDeleteSnapshot = (snapshotName) => {
    setSnapshotToDelete(snapshotName);
    setIsDeleteSnapshotModalOpen(true);
  };

  const confirmDeleteSnapshot = () => {
    if (!snapshotToDelete) return;

    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    invoke('delete_snapshot', { token, masterName: selectedMaster, snapshotName: snapshotToDelete })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      }).finally(() => {
        window.location.reload();
      });
    setIsDeleteSnapshotModalOpen(false);
    setSnapshotToDelete(null);
  };

  const setDefaultMaster = async (masterName) => {
    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    invoke('set_default_master', { token, name: masterName })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      }).finally(() => {
        window.location.reload();
      });
  };

  const confirmDeleteMaster = () => {
    if (!selectedMaster) return;

    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    invoke('delete_master', { token, masterName: selectedMaster })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      }).finally(() => {
        window.location.reload();
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
