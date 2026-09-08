import { ChevronDownIcon } from "lucide-react";
import { cn } from "@/lib/utils";

function NativeSelect({ className, children, size = "default", ...props }) {
  return (
    <div data-slot="native-select-wrapper" className="group/native-select relative w-full has-disabled:opacity-50">
      <select
        data-slot="native-select"
        data-size={size}
        className={cn(
          "h-8 w-full min-w-0 appearance-none rounded-lg border border-input bg-transparent py-1 pl-2.5 pr-8 text-sm outline-none transition-colors focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:cursor-not-allowed aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:bg-input/30 data-[size=sm]:h-7",
          className
        )}
        {...props}
      >
        {children}
      </select>
      <ChevronDownIcon aria-hidden="true" className="pointer-events-none absolute top-1/2 right-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
    </div>
  );
}

function NativeSelectOption({ className, ...props }) {
  return <option data-slot="native-select-option" className={cn("bg-popover text-popover-foreground", className)} {...props} />;
}

function NativeSelectOptGroup({ className, ...props }) {
  return <optgroup data-slot="native-select-optgroup" className={cn("bg-popover text-popover-foreground", className)} {...props} />;
}

export { NativeSelect, NativeSelectOption, NativeSelectOptGroup };
