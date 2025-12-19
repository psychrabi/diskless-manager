import { useToastStore } from "@/store/useToastStore";

const typeStyles = {
  success: "bg-green-500 text-white",
  error: "bg-red-500 text-white",
  warning: "bg-yellow-500 text-white",
  info: "bg-blue-500 text-white",
};

const typeIcons = {
  success: "✓",
  error: "✕",
  warning: "⚠",
  info: "ℹ",
};

export default function Toast({ toast }) {
  const { dismiss } = useToastStore();

  return (
    <div
      className={`flex items-center gap-3 px-4 py-3 rounded-lg shadow-lg min-w-[300px] max-w-lg animate-in slide-in-from-right fade-in duration-200 ${
        typeStyles[toast.type]
      }`}
    >
      <span className="text-lg font-bold">{typeIcons[toast.type]}</span>
      <p className="flex-1 text-sm font-medium">{toast.message}</p>
      <button
        type="button"
        className="p-1 hover:bg-white/20 rounded transition-colors"
        onClick={() => dismiss(toast.id)}
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
            d="M6 18L18 6M6 6l12 12"
          />
        </svg>
      </button>
    </div>
  );
}
