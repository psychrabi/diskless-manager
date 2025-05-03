import { useState, useCallback } from 'react';
import { formatBytes, formatDate } from '../utils/helpers';

export const useMasterManager = (masters, refresh) => {
  const [isCreateSnapshotModalOpen, setIsCreateSnapshotModalOpen] = useState(false);
  const [selectedMaster, setSelectedMaster] = useState(null);
  const [newSnapshotName, setNewSnapshotName] = useState('');

  const handleCreateSnapshot = useCallback(async (masterName) => {
    try {
      // TODO: Replace with actual API call
      console.log('Creating snapshot for master:', masterName);
      // Simulate API response
      await new Promise(resolve => setTimeout(resolve, 500));
      refresh();
      setIsCreateSnapshotModalOpen(false);
    } catch (error) {
      console.error('Failed to create snapshot:', error);
      throw error;
    }
  }, [refresh]);

  const handleOpenCreateSnapshotModal = useCallback((master) => {
    setSelectedMaster(master);
    setNewSnapshotName(`${master.name}@${new Date().toISOString().slice(0,10)}`);
    setIsCreateSnapshotModalOpen(true);
  }, []);

  const handleDeleteSnapshot = useCallback(async (snapshotName) => {
    try {
      if (!window.confirm(`Are you sure you want to delete snapshot "${snapshotName}"?`)) return;
      // TODO: Replace with actual API call
      console.log('Deleting snapshot:', snapshotName);
      // Simulate API response
      await new Promise(resolve => setTimeout(resolve, 500));
      refresh();
    } catch (error) {
      console.error('Failed to delete snapshot:', error);
      throw error;
    }
  }, [refresh]);

  const handleCloneSnapshot = useCallback(async (snapshotName) => {
    try {
      const newMasterName = prompt(`Enter name for the new master ZVOL to be cloned from ${snapshotName}:`, `tank/${snapshotName.split('@')[0].split('/')[1]}-clone`);
      if (!newMasterName) return;
      
      // TODO: Replace with actual API call
      console.log('Cloning snapshot:', snapshotName, 'to:', newMasterName);
      // Simulate API response
      await new Promise(resolve => setTimeout(resolve, 500));
      refresh();
    } catch (error) {
      console.error('Failed to clone snapshot:', error);
      throw error;
    }
  }, [refresh]);

  return {
    isCreateSnapshotModalOpen,
    setIsCreateSnapshotModalOpen,
    selectedMaster,
    newSnapshotName,
    setNewSnapshotName,
    handleCreateSnapshot,
    handleDeleteSnapshot,
    handleCloneSnapshot,
    handleOpenCreateSnapshotModal,
    formatBytes,
    formatDate
  };
};
