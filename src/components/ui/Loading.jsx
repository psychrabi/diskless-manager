export const Loading = ({ message = "Loading\u2026", size = "md" }) => {
  const sizeClasses = {
    sm: "h-8 w-8 border-2",
    md: "h-16 w-16 border-4",
    lg: "h-24 w-24 border-4",
  };

  return (
    <div className="min-h-screen min-w-screen bg-base-200 flex flex-col items-center justify-center">
      <div
        className={`animate-spin rounded-full border-t-4 border-b-4 border-primary ${sizeClasses[size]}`}
      />
      {message && (
        <p className="mt-4 text-lg text-base-content/70">
          {message}
        </p>
      )}
    </div>
  );
};
