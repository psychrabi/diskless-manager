const ImagesEmptyState = ({ onImport, onCreate }) => {
  return (
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
          <button className="btn btn-info text-white" onClick={onImport}>
            Import Image
          </button>
          <button className="btn btn-primary" onClick={onCreate}>
            Add Image
          </button>
        </div>
      </div>
    </div>
  );
};

export default ImagesEmptyState;
