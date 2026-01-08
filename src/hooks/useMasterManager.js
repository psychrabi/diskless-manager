import * as api from "@/api/commands";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useCallback, useState } from "react";
import { formatBytes, formatDate } from "../utils/helpers";

export const useMasterManager = () => {
  const [isCreateSnapshotModalOpen, setIsCreateSnapshotModalOpen] =
    useState(false);
  const [selectedMaster, setSelectedMaster] = useState(null);
  const [newSnapshotName, setNewSnapshotName] = useState("");
  const [isCreateMasterModalOpen, setIsCreateMasterModalOpen] = useState(false);
  const [newMasterName, setNewMasterName] = useState("");
  const [newMasterSize, setNewMasterSize] = useState("50G");
  const [isDeleteSnapshotModalOpen, setIsDeleteSnapshotModalOpen] =
    useState(false);
  const [snapshotToDelete, setSnapshotToDelete] = useState(null);
  const [isDeleteMasterModalOpen, setIsDeleteMasterModalOpen] = useState(false);
  const { success, error } = useToastStore();
  const confirm = useConfirm();
  const fetchMasters = useAppStore((state) => state.fetchMasters);

  // --- Master/Snapshot Actions ---
  const handleOpenCreateMasterModal = () => {
    setNewMasterName("");
    setNewMasterSize("50G"); // Reset to default
    setIsCreateMasterModalOpen(true);
  };

  const handleCreateSnapshot = async (snapshotName) => {
    await api.createSnapshot(selectedMaster, snapshotName)
      .then(async (response) => {
        await fetchMasters();
        if (response.message) success(response.message);
      })
      .catch((err) => {
        error(err?.message || String(err));
      });
    setIsCreateSnapshotModalOpen(false);
  };

  const handleDeleteSnapshot = async (snapshot, image) => {
    if (!snapshot || !image) return;
    confirm({
      title: "Delete Snapshot",
      description: `Are you sure you want to delete snapshot "${snapshot}" for image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: "Delete Snapshot",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    })
      .then((ok) => {
        if (!ok) return;
        api.deleteSnapshot(image, snapshot)
          .then(async (response) => {
            await fetchMasters();
            if (response.message) success(response.message);
          })
          .catch((err) => {
            error(err?.error || String(err));
          });
      })
      .catch((err) => {
        console.error("Confirmation dialog error:", err);
      });
  };

  const handleRollbackSnapshot = async (snapshot, image) => {
    if (!snapshot || !image) return;
    confirm({
      title: "Rollback Snapshot",
      description: `Are you sure you want to rollback snapshot "${snapshot}" for image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: "Rollback Snapshot",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    })
      .then((ok) => {
        if (!ok) return;
        api.rollbackImageSnapshot(image, snapshot)
          .then(async (response) => {
            await fetchMasters();
            if (response.message) success(response.message);
          })
          .catch((err) => {
            error(err?.error || String(err));
          });
      })
      .catch((err) => {
        console.error("Confirmation dialog error:", err);
      });
  };

  const setDefaultMaster = async (masterName) => {
    api.setDefaultImage(masterName)
      .then(async (response) => {
        await fetchMasters();
        if (response.message) success(response.message);
      })
      .catch((err) => {
        error(err?.error || String(err));
      });
  };

  const handleDeleteImage = async (image) => {
    if (!image) return;
    confirm({
      title: "Delete Image",
      description: `Are you sure you want to delete image "${image.name}"? 
      This action cannot be undone and might affect clones.`,
      confirmText: "Delete Image",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    })
      .then(async (ok) => {
        if (!ok) return;

        try {
          await api.deleteImage(image.id);
          success("Image Management", "Image deleted successfully");
          await fetchMasters();
        } catch (err) {
          error(err?.error || "Unknown error");
        }
      })
      .catch((err) => {
        console.error("Confirmation dialog error:", err);
      });
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
    handleRollbackSnapshot,
  };
};
