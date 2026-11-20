import { cn } from "@/lib/utils";

export const PageHeader = ({
  title,
  description,
  actions,
  children,
  className = "",
}) => {
  return (
    <div
      className={cn(
        "mb-2 flex flex-col justify-between gap-4 sm:mb-3 sm:flex-row sm:items-end sm:gap-6",
        className
      )}
    >
      <div className="min-w-0 space-y-2">
        <h1 className="text-balance text-[30px] leading-tight font-semibold tracking-[-0.035em] text-foreground sm:text-[36px]">
          {title}
        </h1>
        {description && (
          <p className="max-w-2xl text-pretty text-sm leading-6 text-muted-foreground">{description}</p>
        )}
        {children}
      </div>
      {actions && (
        <div className="flex w-full flex-wrap items-center gap-2 sm:w-auto sm:shrink-0 sm:justify-end [&>button]:min-h-9 [&>button]:flex-1 sm:[&>button]:flex-none">
          {actions}
        </div>
      )}
    </div>
  );
};
