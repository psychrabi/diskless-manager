import React from 'react';
import { Button, Modal } from '../ui';
import { useNotification } from '@/contexts/NotificationContext';
import { invoke } from '@tauri-apps/api/core';

const RollbackSnapshotConfirmModal = ({ open, setOpen, selectedSnapshot, selectedImage, refresh }) => {
  const { showNotification } = useNotification();

  const confirmRollback = async () => {
    if (!selectedSnapshot || !selectedImage) return;
    await invoke('rollback_master_snapshot', { masterName: selectedImage, snapshotName: selectedSnapshot })
      .then((response) => {
        setOpen(false);
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error');
      }).finally(() => {
        refresh && refresh();
      });
  };

  return (
    <Modal isOpen={open} onClose={() => setOpen(false)} title="Rollback Snapshot" size="2xl">
      <div className="space-y-4">
        <p>
          Are you sure you want to rollback snapshot "{selectedSnapshot}" for image "{selectedImage}"?
          This will revert the master image and re-create all client clones that were using this snapshot.
        </p>
        <div className="flex justify-end space-x-3">
          <Button variant="primary" onClick={confirmRollback}>
            Rollback Snapshot
          </Button>
          <Button variant="destructive" onClick={() => setOpen(false)} >
            Cancel
          </Button>
        </div>
      </div>
    </Modal>
  );
};

export default RollbackSnapshotConfirmModal;
