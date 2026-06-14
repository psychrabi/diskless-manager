export const Error = ({ error }) => {
  const toMessage = (err) => {
    if (err == null) return "";
    if (typeof err === "string") return err;
    if (Array.isArray(err)) return err.map(toMessage).join(", ");
    if (typeof err?.message === "string") return err.message;
    try {
      return String(err);
    } catch {
      /* ignore */
    }
    try {
      return JSON.stringify(err);
    } catch {
      /* ignore */
    }
    return "Unknown error";
  };

  const message = toMessage(error);
  if (!message) return null;

  return (
    <div
      className="bg-error/10 border border-error/30 text-error px-4 py-3 rounded-xl relative mb-6"
      role="alert"
    >
      <strong className="font-bold mr-2">Error:</strong>
      <span className="block sm:inline">{message}</span>
    </div>
  );
};
