import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import {
  importImage,
  deleteImage,
  createSnapshot,
} from "@/api/commands";
import { useToastStore } from "@/store/useToastStore";
import { Modal } from "@/components/ui";
import CreateImageForm from "./CreateImageForm";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import CreateCloneForm from "./CreateCloneForm";

export default function Images() {
  const { images, fetchImages } = useAppStore();
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [showImportModal, setShowImportModal] = useState(false);
  const [showCloneModal, setShowCloneModal] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [selectedImage, setSelectedImage] = useState(null);
  const { success, error } = useToastStore();
  const confirm = useConfirm();

  const [importForm, setImportForm] = useState({
    name: "",
    source_path: "",
    os_type: "linux",
    description: "",
  });

  useEffect(() => {
    fetchImages();
  }, []);

  function formatDate(dateStr) {
    return new Date(dateStr).toLocaleDateString();
  }

  function openImportModal() {
    setImportForm({
      name: "",
      source_path: "",
      os_type: "linux",
      description: "",
    });
    setShowImportModal(true);
  }

  function openCloneModal(image) {
    setSelectedImage(image);
    setShowCloneModal(true);
  }

  async function selectFile() {
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
        setImportForm({ ...importForm, source_path: selected });
      }
    } catch (e) {
      error(`Failed to open file dialog: ${e}`);
    }
  }

  async function handleImport() {
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
      success(
        "Image Management",
        `Image "${importForm.name}" imported successfully`
      );
      setShowImportModal(false);
      await fetchImages();
    } catch (e) {
      error("Image Management", `Failed to import image: ${e}`);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleSnapshot(image) {
    const snapshotName = prompt(
      "Enter snapshot name:",
      `${image.name}-snapshot`
    );
    if (!snapshotName) return;

    try {
      await createSnapshot(image.id, snapshotName);
      success("Image Management", `Snapshot "${snapshotName}" created`);
      await fetchImages();
    } catch (e) {
      error("Image Management", `Failed to create snapshot: ${e}`);
    }
  }

  async function handleDelete(image) {
    confirm({
      title: "Delete Image",
      description: `Are you sure you want to delete image "${image.name}"? This action cannot be undone and might affect clones.`,
      confirmText: "Delete Image",
      cancelText: "Cancel",
      confirmVariant: "success",
      size: "2xl",
    })
      .then(async (ok) => {
        if (!ok) return;
        await deleteImage(image.id);
        success("Image Management", `Image "${image.name}" deleted`);
        await fetchImages();
      })
      .catch((err) => {
        console.error("Confirmation dialog error:", err);
      });
  }

  return (
    <div className="p-6 space-y-8">
      {/* Header */}
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
        <div className="card bg-base-100 shadow-xl border border-base-200/50">
          <div className="card-body items-center text-center p-12">
            <div className="w-20 h-20 bg-base-200 rounded-full flex items-center justify-center text-4xl mb-4">
              💿
            </div>
            <h2 className="card-title text-2xl mb-2">No Images Available</h2>
            <p className="text-base-content/60 max-w-md mb-6">
              Upload or create your first boot image to get started.
            </p>
            <div className="flex gap-4">
              <button
                className="btn btn-info text-white"
                onClick={openImportModal}
              >
                Import Image
              </button>
              <button
                className="btn btn-primary"
                onClick={() => setShowCreateModal(true)}
              >
                Add Image
              </button>
            </div>
          </div>
        </div>
      ) : (
        <div className="card bg-base-100 shadow-xl border border-base-200/50 overflow-visible">
          <div className="overflow-x-auto rounded-xl">
            <table className="table table-zebra w-full">
              <thead className="bg-base-200/50 text-base-content/70">
                <tr>
                  <th>Name</th>
                  <th>OS Type</th>
                  <th>Size (GB)</th>
                  <th>Format</th>
                  <th>Created</th>
                  <th className="text-right">Actions</th>
                </tr>
              </thead>
              <tbody>
                {images.map((image) => (
                  <tr key={image.id} className="hover">
                    <td className="font-bold">{image.name}</td>
                    <td>
                      <div className="badge badge-outline gap-2 capitalize">
                        {image.os_type}
                      </div>
                    </td>
                    <td className="font-mono opacity-70">{image.size_gb} GB</td>
                    <td className="uppercase text-xs font-bold opacity-60">
                      {image.format}
                    </td>
                    <td className="opacity-70">
                      {formatDate(image.created_at)}
                    </td>
                    <td>
                      <div className="flex items-center justify-end gap-2">
                        <button
                          className="btn btn-square btn-sm btn-ghost text-base-content/70 hover:text-primary hover:bg-primary/10"
                          onClick={() => openCloneModal(image)}
                          title="Clone"
                        >
                          <svg
                            className="w-4 h-4"
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                          >
                            <path
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              strokeWidth="2"
                              d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                            />
                          </svg>
                        </button>
                        <button
                          className="btn btn-square btn-sm btn-ghost text-base-content/70 hover:text-warning hover:bg-warning/10"
                          onClick={() => handleSnapshot(image)}
                          title="Snapshot"
                        >
                          <svg
                            className="w-4 h-4"
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                          >
                            <path
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              strokeWidth="2"
                              d="M3 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 0110.07 4h3.86a2 2 0 011.664.89l.812 1.22A2 2 0 0018.07 7H19a2 2 0 012 2v9a2 2 0 01-2 2H5a2 2 0 01-2-2V9z"
                            />
                            <path
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              strokeWidth="2"
                              d="M15 13a3 3 0 11-6 0 3 3 0 016 0z"
                            />
                          </svg>
                        </button>
                        <button
                          className="btn btn-square btn-sm btn-ghost text-error/70 hover:text-error hover:bg-error/10"
                          onClick={() => handleDelete(image)}
                          title="Delete"
                        >
                          <svg
                            className="w-4 h-4"
                            fill="none"
                            stroke="currentColor"
                            viewBox="0 0 24 24"
                          >
                            <path
                              strokeLinecap="round"
                              strokeLinejoin="round"
                              strokeWidth="2"
                              d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                            />
                          </svg>
                        </button>
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Create Image Modal */}
      <CreateImageForm
        show={showCreateModal}
        onClose={() => setShowCreateModal(false)}
      />

      {/* Import Image Modal */}
      <Modal
        title="Import Image"
        show={showImportModal}
        onClose={() => setShowImportModal(false)}
      >
        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            handleImport();
          }}
        >
          <fieldset className="fieldset">
            <div className="label">
              <span className="label-text">
                Name <span className="text-error">*</span>
              </span>
            </div>
            <input
              id="import-name"
              type="text"
              value={importForm.name}
              onChange={(e) =>
                setImportForm({ ...importForm, name: e.target.value })
              }
              className="input input-bordered w-full"
              placeholder="imported-image"
              required
            />
          </fieldset>

          <fieldset className="fieldset">
            <div className="label">
              <span className="label-text">
                Source File <span className="text-error">*</span>
              </span>
            </div>
            <div className="join w-full">
              <input
                id="import-source"
                type="text"
                value={importForm.source_path}
                onChange={(e) =>
                  setImportForm({
                    ...importForm,
                    source_path: e.target.value,
                  })
                }
                className="input input-bordered join-item w-full"
                placeholder="/path/to/image"
                required
              />
              <button
                type="button"
                className="btn btn-neutral join-item"
                onClick={selectFile}
              >
                Browse
              </button>
            </div>
          </fieldset>

          <fieldset className="fieldset">
            <div className="label">
              <span className="label-text">Operating System</span>
            </div>
            <select
              id="import-os"
              value={importForm.os_type}
              onChange={(e) =>
                setImportForm({ ...importForm, os_type: e.target.value })
              }
              className="select select-bordered w-full"
            >
              <option value="linux">Linux</option>
              <option value="windows">Windows</option>
            </select>
          </fieldset>

          <fieldset className="fieldset">
            <div className="label">
              <span className="label-text">Description</span>
            </div>
            <textarea
              id="import-desc"
              value={importForm.description}
              onChange={(e) =>
                setImportForm({ ...importForm, description: e.target.value })
              }
              className="textarea textarea-bordered h-24 w-full"
              placeholder="Optional description..."
            ></textarea>
          </fieldset>

          <div className="flex justify-end gap-3 pt-4 mt-2 border-t border-base-200">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={() => setShowImportModal(false)}
              disabled={submitting}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={submitting}
            >
              {submitting && (
                <span className="loading loading-spinner loading-sm"></span>
              )}
              Import Image
            </button>
          </div>
        </form>
      </Modal>

      {/* Clone Image Modal */}
      <CreateCloneForm
        show={showCloneModal}
        onClose={() => setShowCloneModal(false)}
        selectedImage={selectedImage}
      />
    </div>
  );
}
