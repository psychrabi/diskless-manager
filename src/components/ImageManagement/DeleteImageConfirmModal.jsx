import React from 'react';
import { Button, Modal } from '../ui';
import { invoke } from '@tauri-apps/api/core';
import { useNotification } from '@/contexts/NotificationContext';

const DeleteImageConfirmModal = ({ openDeleteMasterModal, setOpenDeleteMasterModal, selectedImage }) => {
  const { showNotification } = useNotification();

  const confirmDeleteMaster = async () => {
    if (!selectedImage) return;

    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    await invoke('delete_master', { token, masterName: selectedImage })
      .then((response) => {
        setOpenDeleteMasterModal(false);
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      })
  };

  return (
    <Modal isOpen={openDeleteMasterModal} onClose={() => setOpenDeleteMasterModal(false)} title="Delete Snapshot" size="2xl">
      <div className="space-y-4">
        <p>
          Are you sure you want to delete "{selectedImage}" image? <br />
          This action cannot be undone and might affect clones.
        </p>
        <div className="flex justify-end space-x-3">
          <Button variant="primary" onClick={() => confirmDeleteMaster()} >
            Delete Master
          </Button>
          <Button variant="destructive" onClick={() => setOpenDeleteMasterModal(false)} >
            Cancel
          </Button>
        </div>
      </div>
    </Modal>
  );
};

export default DeleteImageConfirmModal;