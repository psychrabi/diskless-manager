import { cn } from "@/lib/utils";
import { Spinner } from "@/components/ui/spinner";

export const Loading = ({
  message = "Loading\u2026",
  size = "md",
  fullScreen = true,
  className,
}) => {
  const sizeClasses = {
    sm: "size-5",
    md: "size-8",
    lg: "size-12",
  };

  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3",
        fullScreen
          ? "min-h-svh w-full bg-background"
          : "min-h-[220px] w-full py-10",
        className,
      )}
      role="status"
      aria-live="polite"
      aria-busy="true"
    >
      <Spinner className={cn("text-muted-foreground", sizeClasses[size])} />
      {message ? (
        <p className={cn("text-sm text-muted-foreground", fullScreen && "text-base")}>
          {message}
        </p>
      ) : null}
    </div>
  );
};