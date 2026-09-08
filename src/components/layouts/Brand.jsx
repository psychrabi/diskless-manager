import { Shield } from "lucide-react";
import { cn } from "@/lib/utils";

const Brand = ({
  subtitle = "Boot Server Control",
  collapsed = false,
  titleClassName = "",
  subtitleClassName = "",
  className = "",
}) => (
  <div className={cn("flex items-center", collapsed ? "justify-center" : "gap-3", className)}>
    <div className="size-8 bg-primary rounded-lg flex items-center justify-center shrink-0">
      <Shield className="size-5 text-primary-foreground" />
    </div>
    {!collapsed && (
      <div>
        <p className={cn("text-heading-sm font-bold text-foreground", titleClassName)}>
          Diskless Manager
        </p>
        <p className={cn("text-caption text-muted-foreground", subtitleClassName)}>
          {subtitle}
        </p>
      </div>
    )}
  </div>
);

export default Brand;
