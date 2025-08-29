import { invoke } from '@tauri-apps/api/core';
import { useCallback, useState } from 'react';

import { formatBytes, formatDate } from '../utils/helpers';
import { useConfirm } from '@/contexts/confirmDialog';
import { useNotification } from '@/contexts/notification';

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
  const confirm = useConfirm();

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
      await invoke('delete_master', { token, masterName: image })
        .then((response) => {
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => {
          showNotification(error, 'error',)
        })
    } else {
      showNotification("Image deletion cancelled", 'error',)
    }
  }

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
    cancelDeleteSnapshot,
    cancelDeleteMaster,
    setDefaultMaster,
    handleOpenDeleteMasterModal,
    isDeleteMasterModalOpen,
    handleDeleteImage,
    handleRollbackSnapshot
  };
};
