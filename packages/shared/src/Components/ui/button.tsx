import * as React from "react";
import { Slot } from "@radix-ui/react-slot";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "../../lib/utils";

/// A ghost that keeps its background at rest, for an action that would otherwise read as
/// decoration among neighbouring icons. Exported for buttons built outside `Button`.
export const CHIP = "bg-foreground/[0.10] text-foreground hover:bg-foreground/[0.18]";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md font-cond font-semibold tracking-[-0.01em] transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0 cursor-default select-none",
  {
    variants: {
      variant: {
        // The one action on the screen. If two buttons in a row are `default`, one of
        // them is wrong.
        default:
          "bg-primary text-primary-foreground hover:bg-primary-hover active:bg-primary-press",
        // Supporting actions, which is most of them.
        secondary:
          "bg-secondary text-secondary-foreground hover:bg-foreground/[0.14] active:bg-foreground/[0.10]",
        outline: "border border-input text-foreground hover:bg-foreground/[0.06]",
        ghost:
          "text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground",
        chip: CHIP,
        destructive:
          "border border-destructive/40 text-destructive hover:bg-destructive/[0.10]",
        link: "text-primary underline-offset-4 hover:underline tracking-normal font-sans",
      },
      size: {
        default: "h-9 px-5 text-[14px]",
        sm: "h-8 px-3.5 text-[12.5px]",
        lg: "h-11 px-7 text-[15px] font-bold tracking-[-0.02em]",
        icon: "h-9 w-9 px-0",
      },
    },
    defaultVariants: { variant: "default", size: "default" },
  },
);

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, children, ...props }, ref) => {
    const Comp = asChild ? Slot : "button";
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      >
        {children}
      </Comp>
    );
  },
);
Button.displayName = "Button";

export { Button, buttonVariants };
