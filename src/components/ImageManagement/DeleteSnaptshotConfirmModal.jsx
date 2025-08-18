import React from 'react';
import { Button, Modal } from '../ui';
import { useNotification } from '@/contexts/NotificationContext';
import { invoke } from '@tauri-apps/api/core';

const DeleteSnaptshotConfirmModal = ({ openDeleteSnapshotModal, setOpenDeleteSnapshotModal, selectedSnapshot, selectedImage }) => {
  const { showNotification } = useNotification();

  const confirmDeleteSnapshot = async () => {
    if (!selectedSnapshot) return;

    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    await invoke('delete_snapshot', { token, masterName: selectedImage, snapshotName: selectedSnapshot })
      .then((response) => {
        setOpenDeleteSnapshotModal(false);
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      })
  };

  return (
    <Modal isOpen={openDeleteSnapshotModal} onClose={() => setOpenDeleteSnapshotModal(false)} title="Delete Snapshot" size="2xl">
      <div className="space-y-4">
        <p>
          Are you sure you want to delete snapshot "{selectedSnapshot}"?
          This action cannot be undone and might affect clones.
        </p>
        <div className="flex justify-end space-x-3">
          <Button variant="primary" onClick={confirmDeleteSnapshot}>
            Delete Snapshot
          </Button>
          <Button variant="destructive" onClick={() => setOpenDeleteSnapshotModal(false)} >
            Cancel
          </Button>
        </div>
      </div>
    </Modal>
  );
};

export default DeleteSnaptshotConfirmModal;