import { cn } from "@/lib/utils";

export const LoadingSkeleton = ({ 
  variant = "text", 
  count = 1, 
  className = "",
  width = "full",
  height = "auto"
}) => {
  const variants = {
    text: "skeleton-text h-4",
    heading: "skeleton-text h-6",
    avatar: "skeleton-avatar",
    card: "skeleton h-32",
    button: "skeleton h-10 w-24",
    table: "skeleton-text h-12"
  };

  const widthClasses = {
    full: "w-full",
    "3/4": "w-3/4", 
    "1/2": "w-1/2",
    "1/3": "w-1/3",
    "1/4": "w-1/4"
  };

  const heightClasses = {
    auto: "",
    sm: "h-4",
    md: "h-6", 
    lg: "h-8",
    xl: "h-12"
  };

  const skeletonClass = cn(
    variants[variant],
    widthClasses[width] || width,
    heightClasses[height],
    className
  );

  if (count === 1) {
    return <div className={skeletonClass} aria-hidden="true" />;
  }

  return (
    <div className="space-y-3" aria-hidden="true">
      {Array.from({ length: count }, (_, i) => (
        <div key={i} className={skeletonClass} aria-hidden="true" />
      ))}
    </div>
  );
};

export const TableSkeleton = ({ rows = 5, columns = 4 }) => (
  <div className="space-y-4">
    {/* Header skeleton */}
    <div className="flex space-x-4">
      {Array.from({ length: columns }, (_, i) => (
        <LoadingSkeleton key={i} variant="heading" width="1/4" />
      ))}
    </div>
    
    {/* Row skeletons */}
    {Array.from({ length: rows }, (_, rowIndex) => (
      <div key={rowIndex} className="flex space-x-4">
        {Array.from({ length: columns }, (_, colIndex) => (
          <LoadingSkeleton key={colIndex} variant="text" width="1/4" />
        ))}
      </div>
    ))}
  </div>
);

export const CardSkeleton = ({ showHeader = true, showActions = false }) => (
  <div className="card-professional p-6 space-y-4">
    {showHeader && (
      <div className="flex justify-between items-start">
        <div className="flex items-start space-x-4">
          <LoadingSkeleton variant="avatar" />
          <div className="space-y-2">
            <LoadingSkeleton variant="heading" width="1/2" />
            <LoadingSkeleton variant="text" width="3/4" />
          </div>
        </div>
        {showActions && (
          <LoadingSkeleton variant="button" />
        )}
      </div>
    )}
    <div className="space-y-3">
      <LoadingSkeleton variant="text" count={3} />
    </div>
  </div>
);