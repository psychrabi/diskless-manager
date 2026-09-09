import { cn } from "@/lib/utils";
import { CardDescription } from "@/components/ui/card";

export const PageHeader = ({
  title,
  description,
  actions,
  className = "",
}) => {
  return (
    <div
      className={cn(
        "flex flex-wrap items-end justify-between gap-3",
        className
      )}
    >
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description && <CardDescription>{description}</CardDescription>}
      </div>
      {actions && (
        <div className="flex flex-wrap items-center gap-2">{actions}</div>
      )}
    </div>
  );
};
