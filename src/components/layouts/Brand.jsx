import { Boxes } from "lucide-react";
import { cn } from "@/lib/utils";

const Brand = ({
  subtitle = "Boot server control",
  collapsed = false,
  inverted = false,
  titleClassName = "",
  subtitleClassName = "",
  className = "",
}) => (
  <div className={cn("flex items-center", collapsed ? "justify-center" : "gap-3", className)}>
    <div
      className={cn(
        "flex size-9 shrink-0 items-center justify-center rounded-md border",
        inverted
          ? "border-white/15 bg-white/[0.06] text-white"
          : "border-border bg-card text-foreground"
      )}
    >
      <Boxes className="size-[18px]" aria-hidden="true" />
    </div>
    {!collapsed && (
      <div className="min-w-0">
        <p className={cn("truncate text-sm font-semibold tracking-[-0.015em] text-foreground", titleClassName)}>
          Diskless Manager
        </p>
        <p className={cn("mt-0.5 truncate text-[10px] font-medium tracking-[0.14em] uppercase text-muted-foreground", subtitleClassName)}>
          {subtitle}
        </p>
      </div>
    )}
  </div>
);

export default Brand;
