import { Boxes } from "lucide-react";
import { cn } from "@/lib/utils";

const Brand = ({
  subtitle = "Boot server control",
  collapsed = false,
  titleClassName = "",
  subtitleClassName = "",
  className = "",
}) => (
  <div className={cn("flex items-center", collapsed ? "justify-center" : "gap-3", className)}>
    <div className="relative flex size-9 shrink-0 items-center justify-center rounded-xl bg-gradient-to-br from-primary to-primary/70 shadow-sm ring-1 ring-primary/20">
      <Boxes className="size-[18px] text-primary-foreground" />
      <span className="absolute -right-0.5 -bottom-0.5 size-2.5 rounded-full border-2 border-background bg-success" />
    </div>
    {!collapsed && (
      <div className="min-w-0">
        <p className={cn("truncate text-[15px] font-semibold tracking-[-0.015em] text-foreground", titleClassName)}>
          Diskless Manager
        </p>
        <p className={cn("mt-0.5 truncate text-[11px] text-muted-foreground", subtitleClassName)}>
          {subtitle}
        </p>
      </div>
    )}
  </div>
);

export default Brand;
