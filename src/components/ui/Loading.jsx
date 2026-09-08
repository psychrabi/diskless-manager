import { cn } from "@/lib/utils";
import { Spinner } from "@/components/ui/spinner";

export const Loading = ({ message = "Loading\u2026", size = "md" }) => {
  const sizeClasses = {
    sm: "size-6",
    md: "size-10",
    lg: "size-14",
  };

  return (
    <div className="flex min-h-screen min-w-screen flex-col items-center justify-center gap-4 bg-background">
      <Spinner className={cn(sizeClasses[size])} />
      {message && <p className="text-lg text-muted-foreground">{message}</p>}
    </div>
  );
};