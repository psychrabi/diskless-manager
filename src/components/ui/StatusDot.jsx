import { cn } from "@/lib/utils";

export const StatusDot = ({ running, label, className = "" }) => {
  const status = running ? "Running" : "Stopped";

  return (
    <span
      role="img"
      aria-label={label ? `${label}: ${status}` : status}
      title={status}
      className={cn("relative flex size-2.5 shrink-0", className)}
    >
      {running && (
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-75" />
      )}
      <span
        className={cn(
          "relative inline-flex size-2.5 rounded-full",
          running ? "bg-emerald-500" : "bg-destructive"
        )}
      />
    </span>
  );
};
