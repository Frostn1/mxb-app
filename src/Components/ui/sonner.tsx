import { Toaster as Sonner, type ToasterProps } from "sonner";

/**
 * App toast host — bottom-right, styled to the app design. Colors come from
 * our CSS tokens so it tracks the active theme automatically.
 *
 * Every toast is dismissible: a hover X button (`closeButton`) and swipe (built
 * into sonner, right/down for a bottom-right host). This matters for persistent
 * toasts like a failed install (`duration: Infinity`) that otherwise linger.
 */
export function Toaster(props: ToasterProps) {
  return (
    <Sonner
      position="bottom-right"
      offset={16}
      closeButton
      swipeDirections={["right", "bottom"]}
      toastOptions={{
        classNames: {
          toast:
            "!bg-popover !text-foreground !border !border-input !rounded-xl !shadow-[0_12px_32px_rgba(0,0,0,0.45)] !text-[12.5px] !font-sans",
          title: "!font-bold !text-[12.5px]",
          description: "!text-muted-foreground !text-[11.5px]",
          actionButton: "!bg-primary !text-primary-foreground !rounded-md !text-[11.5px] !font-semibold",
          cancelButton: "!bg-transparent !text-muted-foreground !text-[11.5px]",
          closeButton:
            "!bg-popover !text-muted-foreground hover:!text-foreground !border !border-input !rounded-full",
          error: "!border-destructive/40",
          success: "!border-input",
        },
      }}
      {...props}
    />
  );
}
