import { CircleHelp } from "lucide-react";

import {
  Popover,
  PopoverContent,
  PopoverDescription,
  PopoverHeader,
  PopoverTitle,
  PopoverTrigger,
} from "@/Components/ui/popover";
import { cn } from "@/lib/utils";

interface HelpHintProps {
  /** Short heading — usually the screen name. */
  title: string;
  /** One or two sentences explaining what the screen is for. */
  description: string;
  className?: string;
}

/**
 * A small "?" icon that opens a popover explaining what the current screen
 * does. Drop it into a screen's header, right after the <h1>.
 */
export default function HelpHint({ title, description, className }: HelpHintProps) {
  return (
    <Popover>
      <PopoverTrigger
        aria-label={`About ${title}`}
        className={cn(
          "flex cursor-default items-center text-muted-foreground/70 outline-none transition-colors hover:text-foreground focus-visible:text-foreground",
          className,
        )}
      >
        <CircleHelp className="size-[15px]" />
      </PopoverTrigger>
      <PopoverContent align="start" className="w-64">
        <PopoverHeader>
          <PopoverTitle>{title}</PopoverTitle>
          <PopoverDescription className="text-[12.5px] leading-relaxed">
            {description}
          </PopoverDescription>
        </PopoverHeader>
      </PopoverContent>
    </Popover>
  );
}
