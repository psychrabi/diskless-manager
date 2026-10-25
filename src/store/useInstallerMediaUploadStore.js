import { create } from "zustand";
import { uploadInstallerMedia } from "@/api/modules/installerMedia";
import { useToastStore } from "@/store/useToastStore";

const idleState = {
  status: "idle",
  progress: 0,
  fileName: null,
  displayName: null,
  error: null,
};

// Identifies the newest upload so late callbacks from a reset or replaced
// upload cannot overwrite the state of a newer one.
let latestUpload = 0;

/**
 * Owns installer-media uploads outside the React tree. The upload keeps
 * running and reports completion through the global toast container after the
 * Installer Media page unmounts.
 */
export const useInstallerMediaUploadStore = create((set, get) => ({
  ...idleState,

  start: ({ displayName, mediaType, file }) => {
    if (get().status === "uploading" || get().status === "validating") {
      return false;
    }

    const uploadId = ++latestUpload;
    set({
      ...idleState,
      status: "uploading",
      fileName: file.name,
      displayName,
    });

    return uploadInstallerMedia({
      displayName,
      mediaType,
      file,
      onProgress: (fraction) => {
        if (uploadId !== latestUpload) return;
        const bounded = Math.min(1, Math.max(0, fraction));
        set({
          status: bounded >= 1 ? "validating" : "uploading",
          progress: Math.round(bounded * 100),
        });
      },
    })
      .then(() => {
        if (uploadId !== latestUpload) return false;
        set({ status: "success", progress: 100 });
        useToastStore
          .getState()
          .success("Installer media", `${displayName} uploaded and validated.`);
        return true;
      })
      .catch((err) => {
        if (uploadId !== latestUpload) return false;
        const message = err?.message || "The upload failed.";
        set({ status: "error", error: message });
        useToastStore.getState().error("Installer media", message);
        return false;
      });
  },

  reset: () => {
    latestUpload += 1;
    set({ ...idleState });
  },
}));
