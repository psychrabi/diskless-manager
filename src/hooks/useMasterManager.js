import { invoke } from "@tauri-apps/api/core";
import { useCallback, useState } from "react";

import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
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

  // --- Master/Snapshot Actions ---
  const handleOpenCreateMasterModal = () => {
    setNewMasterName("");
    setNewMasterSize("50G"); // Reset to default
    setIsCreateMasterModalOpen(true);
  };

  const handleCreateMasterSubmit = async (event) => {
    event.preventDefault();
    setIsCreateMasterModalOpen(false); // Close modal
    // Get token from localStorage
    const token = localStorage.getItem("authToken") || "";
    await invoke("create_image", {
      request: { token, name: newMasterName, size: newMasterSize },
    })
      .then((response) => {
        if (response.message) success(response.message);
      })
      .catch((error) => {
        error(error?.message || String(error));
      })
      .finally(() => {
        window.location.reload();
      });
  };

  const handleCreateSnapshot = (snapshotName) => {
    // Get token from localStorage
    const token = localStorage.getItem("authToken") || "";
    invoke("create_snapshot", {
      token,
      masterName: selectedMaster,
      snapshotName,
    })
      .then((response) => {
        if (response.message) success(response.message);
      })
      .catch((error) => {
        error(error?.message || String(error));
      })
      .finally(() => {
        window.location.reload();
      });
    setIsCreateSnapshotModalOpen(false);
  };

  const handleDeleteSnapshot = async (snapshot, image) => {
    if (!snapshot || !image) return;
    const ok = await confirm({
      title: "Delete Snapshot",
      description: `Are you sure you want to delete snapshot "${snapshot}" for image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: "Delete Snapshot",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    });
    if (ok) {
      const token = localStorage.getItem("authToken") || "";
      await invoke("delete_snapshot", {
        token,
        masterName: image,
        snapshotName: snapshot,
      })
        .then((response) => {
          if (response.message) success(response.message);
        })
        .catch((error) => {
          error(error?.error || String(error));
        });
    } else {
      error("Snapshot deletion cancelled");
    }
  };

  const handleRollbackSnapshot = async (snapshot, image) => {
    if (!snapshot || !image) return;
    const ok = await confirm({
      title: "Rollback Snapshot",
      description: `Are you sure you want to rollback snapshot "${snapshot}" for image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: "Rollback Snapshot",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    });
    if (ok) {
      const token = localStorage.getItem("authToken") || "";
      await invoke("rollback_image_snapshot", {
        token,
        masterName: image,
        snapshotName: snapshot,
      })
        .then((response) => {
          if (response.message) success(response.message);
        })
        .catch((error) => {
          error(error?.error || String(error));
        });
    } else {
      error("Snapshot rollback cancelled");
    }
  };

  const setDefaultMaster = async (masterName) => {
    // Get token from localStorage
    const token = localStorage.getItem("authToken") || "";
    invoke("set_default_image", { token, name: masterName })
      .then((response) => {
        if (response.message) success(response.message);
      })
      .catch((error) => {
        error(error?.error || String(error));
      })
      .finally(() => {
        window.location.reload();
      });
  };

  const handleDeleteImage = async (image) => {
    if (!image) return;
    const ok = await confirm({
      title: "Delete Image",
      description: `Are you sure you want to delete image "${image}"? This action cannot be undone and might affect clones.`,
      confirmText: "Delete Image",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    });
    if (ok) {
      const token = localStorage.getItem("authToken") || "";
      console.log("=== JS: Starting invoke for", image);
      try {
        const response = await invoke("delete_image", {
          token,
          masterName: image,
        });
        console.log("=== JS: Invoke resolved with", response);
        if (response.message) success(response.message);
        if (response.error) error(response.error);
      } catch (error) {
        console.error("=== JS: Invoke rejected with", error);
        error(error?.error || "Unknown error");
      }
    } else {
      error("Image deletion cancelled");
    }
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
    cancelDeleteSnapshot,
    cancelDeleteMaster,
    setDefaultMaster,
    handleOpenDeleteMasterModal,
    isDeleteMasterModalOpen,
    handleDeleteImage,
    handleRollbackSnapshot,
  };
};
