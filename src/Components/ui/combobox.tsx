import { useState } from "react";
import { Check, ChevronsUpDown } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "./button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "./command";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";

interface ComboboxProps {
  value: string;
  options: string[];
  onChange: (v: string) => void;
  placeholder?: string;
  /** Amber "missing" styling on the trigger. */
  invalid?: boolean;
  className?: string;
}

/**
 * A searchable **creatable** combobox: click the trigger to see every option, type
 * to filter, and — since the value can be a free-text font or a captured mod name not
 * in the list — commit whatever you typed via the "Use …" row. Built on the shadcn
 * Popover + Command (cmdk) primitives. cmdk lowercases the value it hands `onSelect`,
 * so each item commits its own original-cased string from the closure instead.
 */
export function Combobox({
  value,
  options,
  onChange,
  placeholder = "Stock",
  invalid,
  className,
}: ComboboxProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const commit = (v: string) => {
    onChange(v);
    setQuery("");
    setOpen(false);
  };

  const q = query.trim();
  const canCreate = q.length > 0 && !options.some((o) => o.toLowerCase() === q.toLowerCase());

  return (
    <Popover
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) setQuery("");
      }}
    >
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="outline"
          role="combobox"
          aria-expanded={open}
          className={cn(
            "h-8 w-full justify-between px-2 text-[12.5px] font-normal",
            !value && "text-muted-foreground",
            invalid && "border-amber-500/40",
            className,
          )}
        >
          <span className="truncate">{value || placeholder}</span>
          <ChevronsUpDown className="ml-1 h-3.5 w-3.5 flex-none opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[--radix-popover-trigger-width] p-0" align="start">
        <Command>
          <CommandInput
            placeholder="Search…"
            value={query}
            onValueChange={setQuery}
            className="text-[12.5px]"
          />
          <CommandList>
            {!canCreate && <CommandEmpty>No matches.</CommandEmpty>}
            <CommandGroup>
              {options.map((o) => (
                <CommandItem
                  key={o}
                  value={o}
                  onSelect={() => commit(o)}
                  className="text-[12.5px]"
                >
                  <Check
                    className={cn("mr-2 h-3.5 w-3.5 flex-none", o === value ? "opacity-100" : "opacity-0")}
                  />
                  <span className="truncate">{o}</span>
                </CommandItem>
              ))}
              {canCreate && (
                <CommandItem
                  // Keep it visible under cmdk's own filter (which matches on value).
                  value={`__use__ ${q}`}
                  onSelect={() => commit(q)}
                  className="text-[12.5px]"
                >
                  <Check className="mr-2 h-3.5 w-3.5 flex-none opacity-0" />
                  Use “{q}”
                </CommandItem>
              )}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
