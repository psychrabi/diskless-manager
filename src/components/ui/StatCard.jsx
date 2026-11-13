import { cn } from "@/lib/utils";

const TONES = {
  primary: "border-primary/15 bg-primary/10 text-primary",
  success: "border-success/15 bg-success/10 text-success",
  warning: "border-warning/20 bg-warning/10 text-warning",
  info: "border-info/15 bg-info/10 text-info",
  muted: "border-border/80 bg-muted text-muted-foreground",
};

export function StatCard({
  icon: Icon,
  label,
  value,
  hint,
  tone = "primary",
  progress,
  className = "",
}) {
  const pct = progress == null ? null : Math.max(0, Math.min(100, Number(progress)));

  return (
    <div
      className={cn(
        "min-w-0 bg-card p-4 sm:p-5",
        className
      )}
    >
      <div className="flex items-start justify-between gap-2 sm:gap-4">
        <div className="min-w-0">
          <p className="text-[11px] font-medium text-muted-foreground">
            {label}
          </p>
          <p className="mt-2 text-[28px] leading-none font-semibold tracking-[-0.03em] tabular-nums text-foreground">
            {value}
          </p>
          {hint && <p className="mt-2 line-clamp-2 text-xs leading-4 text-muted-foreground">{hint}</p>}
        </div>
        {Icon && (
          <span
            className={cn(
              "flex size-8 shrink-0 items-center justify-center rounded-lg border sm:size-9",
              TONES[tone] || TONES.primary
            )}
          >
            <Icon className="size-4" aria-hidden="true" />
          </span>
        )}
      </div>
      {pct != null && (
        <div className="mt-3 h-1 overflow-hidden rounded-full bg-muted">
          <div
            className={cn(
              "h-full rounded-full transition-[width] duration-500",
              tone === "success" && "bg-success",
              tone === "warning" && "bg-warning",
              tone === "info" && "bg-info",
              (tone === "primary" || !["success", "warning", "info"].includes(tone)) && "bg-primary"
            )}
            style={{ width: `${pct}%` }}
          />
        </div>
      )}
    </div>
  );
}
