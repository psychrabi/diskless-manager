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
        "flex flex-col justify-between gap-5 border-b border-border/70 pb-5 sm:flex-row sm:items-end",
        className
      )}
    >
      <div className="min-w-0 space-y-1.5">
        <h1 className="text-[28px] leading-[1.1] font-semibold tracking-[-0.035em] text-foreground sm:text-[32px]">
          {title}
        </h1>
        {description && (
          <p className="max-w-2xl text-sm leading-6 text-muted-foreground">{description}</p>
        )}
        {children}
      </div>
      {actions && (
        <div className="flex w-full flex-wrap items-center gap-2 sm:w-auto sm:justify-end [&>button]:flex-1 sm:[&>button]:flex-none">
          {actions}
        </div>
      )}
    </div>
  );
};
