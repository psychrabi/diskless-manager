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
      className="mb-6 rounded-xl border border-destructive/30 bg-destructive/10 px-4 py-3 text-destructive"
      role="alert"
    >
      <strong className="font-bold mr-2">Error:</strong>
      <span className="block sm:inline">{message}</span>
    </div>
  );
};