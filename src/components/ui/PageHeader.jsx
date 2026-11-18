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
        "mb-6 flex flex-col justify-between gap-4 border-b border-border pb-5 sm:mb-7 sm:flex-row sm:items-end sm:gap-6",
        className
      )}
    >
      <div className="min-w-0 space-y-2">
        <h1 className="text-balance text-[30px] leading-[1.02] font-semibold tracking-[-0.045em] text-foreground sm:text-[38px] xl:text-[42px]">
          {title}
        </h1>
        {description && (
          <p className="max-w-2xl text-pretty text-[13px] leading-6 text-muted-foreground xl:text-[15px] xl:leading-7">{description}</p>
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
