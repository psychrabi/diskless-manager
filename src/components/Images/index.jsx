import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { createSnapshot, deleteImage, importImage } from "@/api/commands";
import { useConfirm } from "@/contexts/confirmDialog";
import { useToastStore } from "@/store/useToastStore";
import { useAppStore } from "@/store/useAppStore";
import CreateCloneForm from "./CreateCloneForm";
import CreateImageForm from "./CreateImageForm";
import ImageTable from "./ImageTable";
import ImagesEmptyState from "./ImagesEmptyState";
import ImportImageModal from "./ImportImageModal";

const INITIAL_IMPORT_FORM = {
  name: "",
  source_path: "",
  os_type: "linux",
  description: "",
};

export default function Images() {
  const { images, fetchImages } = useAppStore();
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [showImportModal, setShowImportModal] = useState(false);
  const [showCloneModal, setShowCloneModal] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [selectedImage, setSelectedImage] = useState(null);
  const [importForm, setImportForm] = useState(INITIAL_IMPORT_FORM);
  const { success, error } = useToastStore();
  const confirm = useConfirm();

  useEffect(() => {
    fetchImages();
  }, [fetchImages]);

  const formatDate = useCallback((dateStr) => {
    return new Date(dateStr).toLocaleDateString();
  }, []);

  const updateImportField = useCallback((field, value) => {
    setImportForm((prev) => ({ ...prev, [field]: value }));
  }, []);

  const openImportModal = useCallback(() => {
    setImportForm(INITIAL_IMPORT_FORM);
    setShowImportModal(true);
  }, []);

  const openCloneModal = useCallback((image) => {
    setSelectedImage(image);
    setShowCloneModal(true);
  }, []);

  const selectFile = useCallback(async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Disk Images",
            extensions: ["img", "qcow2", "vmdk", "vdi", "vhd", "vhdx", "iso"],
          },
          { name: "All Files", extensions: ["*"] },
        ],
      });

      if (selected && typeof selected === "string") {
        setImportForm((prev) => ({ ...prev, source_path: selected }));
      }
    } catch (e) {
      error(`Failed to open file dialog: ${e}`);
    }
  }, [error]);

  const handleImport = useCallback(async () => {
    setSubmitting(true);
    try {
      const request = {
        name: importForm.name,
        source_path: importForm.source_path,
        os_type: importForm.os_type,
      };
      if (importForm.description) {
        request.description = importForm.description;
      }

      await importImage(request);
      success("Image Management", `Image "${importForm.name}" imported successfully`);
      setShowImportModal(false);
      await fetchImages();
    } catch (e) {
      error("Image Management", `Failed to import image: ${e}`);
    } finally {
      setSubmitting(false);
    }
  }, [error, fetchImages, importForm.description, importForm.name, importForm.os_type, importForm.source_path, success]);

  const handleSnapshot = useCallback(
    async (image) => {
      const snapshotName = prompt("Enter snapshot name:", `${image.name}-snapshot`);
      if (!snapshotName) return;

      try {
        await createSnapshot(image.id, snapshotName);
        success("Image Management", `Snapshot "${snapshotName}" created`);
        await fetchImages();
      } catch (e) {
        error("Image Management", `Failed to create snapshot: ${e}`);
      }
    },
    [error, fetchImages, success]
  );

  const handleDelete = useCallback(
    async (image) => {
      try {
        const ok = await confirm({
          title: "Delete Image",
          description: `Are you sure you want to delete image "${image.name}"? This action cannot be undone and might affect clones.`,
          confirmText: "Delete Image",
          cancelText: "Cancel",
          confirmVariant: "success",
          size: "2xl",
        });

        if (!ok) return;

        await deleteImage(image.id);
        success("Image Management", `Image "${image.name}" deleted`);
        await fetchImages();
      } catch (err) {
        console.error("Image deletion flow error:", err);
      }
    },
    [confirm, fetchImages, success]
  );

  return (
    <div className="p-6 space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-extrabold text-base-content tracking-tight">
            Images
          </h1>
          <p className="text-base-content/60 mt-1 font-medium">
            Manage boot images and ISOs
          </p>
        </div>
        <div className="flex gap-3">
          <button
            className="btn btn-info shadow-md shadow-info/20 text-white"
            onClick={openImportModal}
          >
            <svg
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="2"
                d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"
              />
            </svg>
            Import
          </button>
          <button
            className="btn btn-primary shadow-md shadow-primary/20"
            onClick={() => setShowCreateModal(true)}
          >
            <svg
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth="2"
                d="M12 4v16m8-8H4"
              />
            </svg>
            Add Image
          </button>
        </div>
      </div>

      {images.length === 0 ? (
        <ImagesEmptyState
          onImport={openImportModal}
          onCreate={() => setShowCreateModal(true)}
        />
      ) : (
        <ImageTable
          images={images}
          formatDate={formatDate}
          onClone={openCloneModal}
          onSnapshot={handleSnapshot}
          onDelete={handleDelete}
        />
      )}

      <CreateImageForm
        show={showCreateModal}
        onClose={() => setShowCreateModal(false)}
      />

      <ImportImageModal
        show={showImportModal}
        onClose={() => setShowImportModal(false)}
        form={importForm}
        onChange={updateImportField}
        onBrowse={selectFile}
        onSubmit={handleImport}
        submitting={submitting}
      />

      <CreateCloneForm
        show={showCloneModal}
        onClose={() => setShowCloneModal(false)}
        selectedImage={selectedImage}
      />
    </div>
  );
}
