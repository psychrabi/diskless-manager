export const Loading = ({ message = "Loading...", size = "md" }) => {
  const sizeClasses = {
    sm: "h-8 w-8 border-2",
    md: "h-16 w-16 border-4",
    lg: "h-24 w-24 border-4",
  };

  return (
    <div className="min-h-screen min-w-screen bg-gray-100 dark:bg-gray-900 flex flex-col items-center justify-center">
      <div
        className={`animate-spin rounded-full border-t-4 border-b-4 border-blue-500 ${sizeClasses[size]}`}
      ></div>
      {message && (
        <p className="mt-4 text-lg text-gray-700 dark:text-gray-300">
          {message}
        </p>
      )}
    </div>
  );
};
