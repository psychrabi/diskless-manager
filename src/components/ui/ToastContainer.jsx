import { useToastStore } from "@/store/useToastStore";
import Toast from "./Toast.jsx";

export const ToastContainer = () => {
  const { toasts } = useToastStore();

  return (
    <div className="fixed bottom-4 right-4 z-50 space-y-2 w-[calc(100vw-2rem)] max-w-sm sm:w-auto">
      {toasts.map((toast) => (
        <Toast key={toast.id} toast={toast} />
      ))}
    </div>
  );
};