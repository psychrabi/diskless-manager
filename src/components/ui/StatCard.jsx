import { cn } from "@/lib/utils";

const TONES = {
  primary: "border-primary/25 bg-primary/10 text-primary",
  success: "border-success/25 bg-success/10 text-success",
  warning: "border-warning/30 bg-warning/10 text-warning",
  info: "border-info/25 bg-info/10 text-info",
  muted: "border-border bg-muted text-muted-foreground",
};

const METERS = {
  primary: "bg-primary",
  success: "bg-success",
  warning: "bg-warning",
  info: "bg-info",
  muted: "bg-muted-foreground",
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
        "min-w-0 bg-card p-4 transition-colors duration-150 hover:bg-muted/25 sm:p-5",
        className
      )}
    >
      <div className="flex items-start justify-between gap-2 sm:gap-4">
        <div className="min-w-0">
          <p className="text-[10px] font-semibold tracking-[0.14em] uppercase text-muted-foreground xl:text-[11px]">
            {label}
          </p>
          <p className="mt-2.5 text-[30px] leading-none font-semibold tracking-[-0.04em] tabular-nums text-foreground xl:text-[34px]">
            {value}
          </p>
          {hint && <p className="mt-2 line-clamp-3 text-xs leading-4 text-muted-foreground xl:text-[13px] xl:leading-5">{hint}</p>}
        </div>
        {Icon && (
          <span
            className={cn(
              "flex size-8 shrink-0 items-center justify-center rounded-md border sm:size-9",
              TONES[tone] || TONES.primary
            )}
          >
            <Icon className="size-4" aria-hidden="true" />
          </span>
        )}
      </div>
      {pct != null && (
        <div className="mt-3.5 h-1.5 overflow-hidden bg-muted">
          <div
            className={cn(
              "meter-track h-full transition-[width] duration-500",
              METERS[tone] || METERS.primary
            )}
            style={{ width: `${pct}%` }}
          />
        </div>
      )}
    </div>
  );
}
