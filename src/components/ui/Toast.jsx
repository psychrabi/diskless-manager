import { CheckCircle2, Info, TriangleAlert, XCircle, X } from "lucide-react";
import { useToastStore } from "@/store/useToastStore";

const typeMeta = {
  success: { Icon: CheckCircle2, ring: "text-primary" },
  error: { Icon: XCircle, ring: "text-destructive" },
  warning: { Icon: TriangleAlert, ring: "text-muted-foreground" },
  info: { Icon: Info, ring: "text-muted-foreground" },
};

export default function Toast({ toast }) {
  const { dismiss } = useToastStore();
  const { Icon, ring } = typeMeta[toast.type] || typeMeta.info;

  return (
    <div className="flex w-full max-w-full items-start gap-3 rounded-lg bg-popover px-4 py-3 text-popover-foreground shadow-lg ring-1 ring-foreground/10 backdrop-blur-md">
      <Icon className={ring} />
      <div className="flex-1 min-w-0">
        <h4 className="text-sm font-bold leading-tight">{toast.title}</h4>
        {toast.description && (
          <p className="mt-1 text-xs text-muted-foreground leading-normal">
            {toast.description}
          </p>
        )}
      </div>
      <button
        type="button"
        className="p-1 -mr-1 rounded transition-colors hover:bg-muted"
        onClick={() => dismiss(toast.id)}
        aria-label="Dismiss notification"
      >
        <X className="size-4" />
      </button>
    </div>
  );
}