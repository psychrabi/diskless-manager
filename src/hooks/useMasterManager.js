import { applySnapshot, deleteSnapshot, rollbackImageSnapshot, setDefaultImage, deleteImage } from "@/api/modules/images";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useCallback } from "react";

const restoreImageName = (imageName, snapshotName) => {
  const leaf = String(imageName || "image").split("/").pop();
  const stamp = new Date()
    .toISOString()
    .replace(/[-:]/g, "")
    .replace("T", "-")
    .slice(0, 15);
  return `${leaf}-${snapshotName}-restore-${stamp}`.replace(
    /[^a-zA-Z0-9_-]/g,
    "-"
  );
};

export const useMasterManager = () => {
  const { success, error } = useToastStore();
  const confirm = useConfirm();
  const fetchMasters = useAppStore((state) => state.fetchMasters);

  const handleDeleteSnapshot = useCallback(async (snapshot, image) => {
    if (!snapshot || !image) return;
    try {
      const ok = await confirm({
        title: "Delete Snapshot",
        description: `Are you sure you want to delete snapshot "${snapshot}" for image "${image}"? This action cannot be undone and might affect clones.`,
        confirmText: "Delete Snapshot",
        cancelText: "Cancel",
        confirmVariant: "primary",
        size: "2xl",
      });
      if (!ok) return;

      const response = await deleteSnapshot(image, snapshot);
      await fetchMasters();
      success(
        "Image Management",
        response.message || "Snapshot deleted successfully"
      );
    } catch (err) {
      error(err?.error || err?.message || String(err));
    }
  }, [confirm, fetchMasters, success, error]);

  const handleRollbackSnapshot = useCallback(async (snapshot, image, newerSnapshots = []) => {
    if (!snapshot || !image) return;
    const newerNames = newerSnapshots.map((entry) => entry.name);
    const newerSummary = newerNames.length
      ? `This permanently deletes ${newerNames.length} newer recovery point${
          newerNames.length === 1 ? "" : "s"
        }: ${newerNames.join(", ")}.`
      : "No newer recovery points will be deleted.";
    try {
      const ok = await confirm({
        title: "Rollback Snapshot",
        description: `Restore image "${image}" to snapshot "${snapshot}"? ${newerSummary} Clients assigned to newer recovery points must be reassigned first. This cannot be undone.`,
        confirmText: "Rollback Snapshot",
        cancelText: "Cancel",
        confirmVariant: newerNames.length ? "destructive" : "primary",
        size: "2xl",
      });
      if (!ok) return;
      const response = await rollbackImageSnapshot(image, snapshot, newerNames);
      await fetchMasters();
      if (response.message) success(response.message);
    } catch (err) {
      error(err?.error || err?.message || String(err));
    }
  }, [confirm, fetchMasters, success, error]);

  const handleApplySnapshot = useCallback(async (snapshot, image) => {
    if (!snapshot || !image) return;
    try {
      const answer = await confirm({
        title: "Apply Restore Point",
        description: `Create a new master image from "${snapshot}" of "${image}"? The current image, newer restore points, and all client assignments are left unchanged. Assign clients to the new image afterwards.`,
        confirmText: "Create Image",
        cancelText: "Cancel",
        confirmVariant: "primary",
        size: "2xl",
        showInput: true,
        inputLabel: "New image name",
        defaultValue: restoreImageName(image, snapshot),
      });
      if (typeof answer !== "string") return;
      const newName = answer.trim();
      if (!newName) {
        error("Image Management", "Enter a name for the new image");
        return;
      }

      const created = await applySnapshot(image, snapshot, newName);
      await fetchMasters();
      success(
        "Image Management",
        `Created ${created?.name || newName} from restore point "${snapshot}"`
      );
    } catch (err) {
      error(err?.error || err?.message || String(err));
    }
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
          error(err?.message || err?.error || String(err));
        }
      })
      .catch((err) => {
        console.error("Confirmation dialog error:", err);
      });
  }, [confirm, fetchMasters, success, error]);

  return {
    handleDeleteSnapshot,
    handleRollbackSnapshot,
    handleApplySnapshot,
    setDefaultMaster,
    handleDeleteImage,
  };
};
