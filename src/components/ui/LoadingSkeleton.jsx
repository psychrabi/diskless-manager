import { cn } from "@/lib/utils";
import { Skeleton } from "@/components/ui/skeleton";

export const LoadingSkeleton = ({
  variant = "text",
  count = 1,
  className = "",
  width = "full",
  height = "auto"
}) => {
  const variants = {
    text: "h-4",
    heading: "h-6",
    avatar: "size-10 rounded-full",
    card: "h-32",
    button: "h-10 w-24",
    table: "h-12",
  };

  const widthClasses = {
    full: "w-full",
    "3/4": "w-3/4",
    "1/2": "w-1/2",
    "1/3": "w-1/3",
    "1/4": "w-1/4",
    "4/5": "w-4/5",
    "5/6": "w-5/6",
    "2/3": "w-2/3",
    "3/5": "w-3/5",
  };

  const heightClasses = {
    auto: "",
    sm: "h-4",
    md: "h-6",
    lg: "h-8",
    xl: "h-12",
  };

  const skeletonClass = cn(
    variants[variant],
    widthClasses[width] || width,
    heightClasses[height],
    className
  );

  if (count === 1) {
    return (
      <Skeleton className={cn("rounded-md", skeletonClass)} aria-hidden="true" />
    );
  }

  return (
    <div className="flex flex-col gap-3" aria-hidden="true">
      {Array.from({ length: count }, (_, i) => (
        <Skeleton key={i} className={cn("rounded-md", skeletonClass)} aria-hidden="true" />
      ))}
    </div>
  );
};

export const TableSkeleton = ({ rows = 5, columns = 4 }) => (
  <div className="flex flex-col gap-4">
    <div className="flex gap-4">
      {Array.from({ length: columns }, (_, i) => (
        <LoadingSkeleton key={i} variant="heading" width="1/4" />
      ))}
    </div>

    {Array.from({ length: rows }, (_, rowIndex) => (
      <div key={rowIndex} className="flex gap-4">
        {Array.from({ length: columns }, (_, colIndex) => (
          <LoadingSkeleton key={colIndex} variant="text" width="1/4" />
        ))}
      </div>
    ))}
  </div>
);

export const CardSkeleton = ({ showHeader = true, showActions = false }) => (
  <div className="rounded-xl bg-card p-4 ring-1 ring-foreground/10 flex flex-col gap-4">
    {showHeader && (
      <div className="flex justify-between items-start">
        <div className="flex items-start gap-4">
          <LoadingSkeleton variant="avatar" />
          <div className="flex flex-col gap-2">
            <LoadingSkeleton variant="heading" width="1/2" />
            <LoadingSkeleton variant="text" width="3/4" />
          </div>
        </div>
        {showActions && <LoadingSkeleton variant="button" />}
      </div>
    )}
    <div className="flex flex-col gap-3">
      <LoadingSkeleton variant="text" count={3} />
    </div>
  </div>
);