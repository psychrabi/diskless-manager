import { Modal } from "@/components/ui";

const ImportImageModal = ({
  show,
  onClose,
  form,
  onChange,
  onBrowse,
  onSubmit,
  submitting,
}) => {
  return (
    <Modal title="Import Image" show={show} onClose={onClose}>
      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit();
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
            value={form.name}
            onChange={(e) => onChange("name", e.target.value)}
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
              value={form.source_path}
              onChange={(e) => onChange("source_path", e.target.value)}
              className="input input-bordered join-item w-full"
              placeholder="/path/to/image"
              required
            />
            <button
              type="button"
              className="btn btn-neutral join-item"
              onClick={onBrowse}
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
            value={form.os_type}
            onChange={(e) => onChange("os_type", e.target.value)}
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
            value={form.description}
            onChange={(e) => onChange("description", e.target.value)}
            className="textarea textarea-bordered h-24 w-full"
            placeholder="Optional description..."
          ></textarea>
        </fieldset>

        <div className="flex justify-end gap-3 pt-4 mt-2 border-t border-base-200">
          <button
            type="button"
            className="btn btn-ghost"
            onClick={onClose}
            disabled={submitting}
          >
            Cancel
          </button>
          <button type="submit" className="btn btn-primary" disabled={submitting}>
            {submitting && (
              <span className="loading loading-spinner loading-sm"></span>
            )}
            Import Image
          </button>
        </div>
      </form>
    </Modal>
  );
};

export default ImportImageModal;
