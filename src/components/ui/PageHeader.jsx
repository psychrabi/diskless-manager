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
        "flex flex-col items-stretch justify-between gap-3 sm:flex-row sm:items-end",
        className
      )}
    >
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description && <CardDescription>{description}</CardDescription>}
      </div>
      {actions && (
        <div className="flex w-full flex-wrap items-center gap-2 sm:w-auto [&>button]:flex-1 sm:[&>button]:flex-none">{actions}</div>
      )}
    </div>
  );
};
