const IconButton = ({ title, className, onClick, children }) => (
  <button
    className={`btn btn-square btn-sm btn-ghost ${className}`}
    onClick={onClick}
    title={title}
  >
    {children}
  </button>
);

const ImageTable = ({ images, formatDate, onClone, onSnapshot, onDelete }) => {
  return (
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
                <td className="opacity-70">{formatDate(image.created_at)}</td>
                <td>
                  <div className="flex items-center justify-end gap-2">
                    <IconButton
                      className="text-base-content/70 hover:text-primary hover:bg-primary/10"
                      onClick={() => onClone(image)}
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
                    </IconButton>
                    <IconButton
                      className="text-base-content/70 hover:text-warning hover:bg-warning/10"
                      onClick={() => onSnapshot(image)}
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
                    </IconButton>
                    <IconButton
                      className="text-error/70 hover:text-error hover:bg-error/10"
                      onClick={() => onDelete(image)}
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
                    </IconButton>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
};

export default ImageTable;
