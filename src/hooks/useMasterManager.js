import { createSnapshot, deleteSnapshot, rollbackImageSnapshot, setDefaultImage, deleteImage } from "@/api/modules/images";
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
  const handleOpenCreateMasterModal = useCallback(() => {
    setNewMasterName("");
    setNewMasterSize("50G"); // Reset to default
    setIsCreateMasterModalOpen(true);
  }, []);

  const handleCreateSnapshot = useCallback(async (snapshotName) => {
    await createSnapshot(selectedMaster, snapshotName)
      .then(async (response) => {
        await fetchMasters();
        if (response.message) success(response.message);
      })
      .catch((err) => {
        error(err?.message || String(err));
      });
    setIsCreateSnapshotModalOpen(false);
  }, [selectedMaster, fetchMasters, success, error]);

  const handleDeleteSnapshot = useCallback(async (snapshot, image) => {
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
        deleteSnapshot(image, snapshot)
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
  }, [confirm, fetchMasters, success, error]);

  const handleRollbackSnapshot = useCallback(async (snapshot, image) => {
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
        rollbackImageSnapshot(image, snapshot)
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
  }, [confirm, fetchMasters, success, error]);

  const setDefaultMaster = useCallback(async (masterName) => {
    setDefaultImage(masterName)
      .then(async (response) => {
        await fetchMasters();
        if (response.message) success(response.message);
      })
      .catch((err) => {
        error(err?.error || String(err));
      });
  }, [fetchMasters, success, error]);

  const handleDeleteImage = useCallback(async (image) => {
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
          await deleteImage(image.id);
          success("Image Management", "Image deleted successfully");
          await fetchMasters();
        } catch (err) {
          error(err?.error || "Unknown error");
        }
      })
      .catch((err) => {
        console.error("Confirmation dialog error:", err);
      });
  }, [confirm, fetchMasters, success, error]);

  const handleOpenDeleteMasterModal = useCallback((master) => {
    setSelectedMaster(master);
    setIsDeleteMasterModalOpen(true);
  }, []);

  const cancelDeleteMaster = useCallback(() => {
    setIsDeleteMasterModalOpen(false);
    setSelectedMaster(null);
  }, []);

  const cancelDeleteSnapshot = useCallback(() => {
    setIsDeleteSnapshotModalOpen(false);
    setSnapshotToDelete(null);
  }, []);

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
