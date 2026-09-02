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
    <div className="w-8 h-8 bg-primary rounded-lg flex items-center justify-center shrink-0">
      <Shield className="h-5 w-5 text-primary-content" />
    </div>
    {!collapsed && (
      <div>
        <h1 className={cn("text-heading-sm font-bold text-base-content", titleClassName)}>
          Diskless Manager
        </h1>
        <p className={cn("text-caption text-base-content/60", subtitleClassName)}>
          {subtitle}
        </p>
      </div>
    )}
  </div>
);

export default Brand;