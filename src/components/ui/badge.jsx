import { mergeProps } from "@base-ui/react/merge-props"
import { useRender } from "@base-ui/react/use-render"
import { cva } from "class-variance-authority";
import { cn } from "cn"

const badgeVariants = cva(
  "group/badge inline-flex h-5.5 w-fit shrink-0 items-center justify-center gap-1 overflow-hidden rounded-md border px-2 py-0.5 text-[11px] font-medium tracking-[0.01em] whitespace-nowrap transition-colors focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/50 has-data-[icon=inline-end]:pr-1.5 has-data-[icon=inline-start]:pl-1.5 aria-invalid:border-destructive aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 [&>svg]:pointer-events-none [&>svg]:size-3!",
  {
    variants: {
      variant: {
        default: "border-primary/15 bg-primary/10 text-primary",
        secondary:
          "border-border/80 bg-secondary text-secondary-foreground",
        destructive:
          "border-destructive/15 bg-destructive/10 text-destructive focus-visible:ring-destructive/20 dark:bg-destructive/15",
        success:
          "border-success/15 bg-success/10 text-success focus-visible:ring-success/20",
        warning:
          "border-warning/20 bg-warning/10 text-warning focus-visible:ring-warning/20",
        info: "border-info/15 bg-info/10 text-info focus-visible:ring-info/20",
        outline:
          "border-border/90 bg-card text-muted-foreground",
        ghost:
          "border-transparent text-muted-foreground hover:bg-muted",
        link: "border-transparent text-primary underline-offset-4 hover:underline",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

function Badge({
  className,
  variant = "default",
  render,
  ...props
}) {
  return useRender({
    defaultTagName: "span",
    props: mergeProps({
      className: cn(badgeVariants({ variant }), className),
    }, props),
    render,
    state: {
      slot: "badge",
      variant,
    },
  });
}

export { Badge, badgeVariants }
