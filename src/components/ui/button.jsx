import { Button as ButtonPrimitive } from "@base-ui/react/button"
import { cva } from "class-variance-authority";
import { cn } from "cn"

const buttonVariants = cva(
  "group/button inline-flex flex-row shrink-0 items-center justify-center gap-1.5 rounded-lg border border-transparent bg-clip-padding text-[13px] font-medium whitespace-nowrap transition-[color,background-color,border-color,box-shadow,transform] outline-none select-none focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/35 focus-visible:ring-offset-2 focus-visible:ring-offset-background active:not-aria-[haspopup]:translate-y-px disabled:pointer-events-none disabled:opacity-45 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 dark:aria-invalid:border-destructive/50 dark:aria-invalid:ring-destructive/40 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground shadow-sm hover:bg-primary/90",
        outline:
          "border-border/90 bg-card text-foreground shadow-[0_1px_1px_rgb(0_0_0/0.03)] hover:bg-muted/70 hover:text-foreground aria-expanded:bg-muted",
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-[color-mix(in_oklch,var(--secondary),var(--foreground)_5%)] aria-expanded:bg-secondary",
        ghost:
          "text-muted-foreground hover:bg-muted/70 hover:text-foreground aria-expanded:bg-muted",
        destructive:
          "border-destructive/15 bg-destructive/10 text-destructive hover:bg-destructive/15 focus-visible:border-destructive/40 focus-visible:ring-destructive/20 dark:bg-destructive/15 dark:hover:bg-destructive/20",
        success:
          "border-success/20 bg-success/10 text-success hover:bg-success/15 focus-visible:border-success/40 focus-visible:ring-success/20",
        warning:
          "border-warning/25 bg-warning/10 text-warning hover:bg-warning/15 focus-visible:border-warning/40 focus-visible:ring-warning/20",
        info:
          "border-info/20 bg-info/10 text-info hover:bg-info/15 focus-visible:border-info/40 focus-visible:ring-info/20",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 px-3.5",
        xs: "h-7 gap-1 rounded-md px-2 text-[11px]",
        sm: "h-8 gap-1 rounded-lg px-3 text-xs",
        lg: "h-10 gap-2 rounded-lg px-4 text-sm",
        icon: "size-9",
        "icon-xs": "size-7 rounded-md",
        "icon-sm": "size-8 rounded-lg",
        "icon-lg": "size-10 rounded-lg",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

function Button({
  className,
  variant = "default",
  size = "default",
  ...props
}) {
  return (
    <ButtonPrimitive
      data-slot="button"
      className={cn(buttonVariants({ variant, size, className }))}
      {...props}
    />
  )
}

export { Button, buttonVariants }
