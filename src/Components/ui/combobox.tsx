import { useState } from "react";
import { Check, ChevronsUpDown } from "lucide-react";
import { cn } from "@/lib/utils";
import { useT } from "@/i18n/context";
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
  /**
   * Offer the "Use …" row that commits whatever was typed. On by default.
   *
   * Off for a slot whose value has to name something that exists — a bike id the backend
   * resolves a model out of, where a typed-in name is only ever a preview that fails.
   */
  allowCreate?: boolean;
  /** Offer the row that clears the slot back to empty/stock. On by default. */
  allowEmpty?: boolean;
  className?: string;
}

/**
 * A searchable **creatable** combobox: click the trigger to see every option, type
 * to filter, and — since the value can be a free-text font or a captured mod name not
 * in the list — commit whatever you typed via the "Use …" row. The first row clears the
 * slot back to the empty/stock value, which no amount of typing can produce. Built on the shadcn
 * Popover + Command (cmdk) primitives. cmdk lowercases the value it hands `onSelect`,
 * so each item commits its own original-cased string from the closure instead.
 *
 * Both of those extra rows are for a *slot*, where an unknown name and an empty value are
 * both legitimate answers. `allowCreate` and `allowEmpty` turn them off for a picker that
 * has to name one of the things in the list — leaving a plain searchable select.
 */
export function Combobox({
  value,
  options,
  onChange,
  placeholder,
  invalid,
  allowCreate = true,
  allowEmpty = true,
  className,
}: ComboboxProps) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");

  const commit = (v: string) => {
    onChange(v);
    setQuery("");
    setOpen(false);
  };

  const q = query.trim();
  const canCreate =
    allowCreate && q.length > 0 && !options.some((o) => o.toLowerCase() === q.toLowerCase());

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
          <span className="truncate">{value || placeholder || t("locker.stock")}</span>
          <ChevronsUpDown className="ml-1 h-3.5 w-3.5 flex-none opacity-50" />
        </Button>
      </PopoverTrigger>
      <PopoverContent className="w-[--radix-popover-trigger-width] p-0" align="start">
        <Command>
          <CommandInput
            placeholder={t("combobox.search")}
            value={query}
            onValueChange={setQuery}
            className="text-[12.5px]"
          />
          <CommandList>
            {!canCreate && <CommandEmpty>{t("library.noMatches")}</CommandEmpty>}
            <CommandGroup>
              {/*
                Empty is a real value — it's what the game writes for an unmodded slot,
                and the trigger already renders it as the placeholder. Without this row
                a slot is one-way: once set, nothing typeable commits "" again.
                The extra words in `value` are just cmdk filter fodder.
              */}
              {allowEmpty && (
                <CommandItem
                  value={`__stock__ ${placeholder} none clear`}
                  onSelect={() => commit("")}
                  className="text-[12.5px]"
                >
                  <Check
                    className={cn("mr-2 h-3.5 w-3.5 flex-none", !value ? "opacity-100" : "opacity-0")}
                  />
                  <span className="truncate text-muted-foreground">{placeholder} (none)</span>
                </CommandItem>
              )}
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
                  {t("combobox.use", { value: q })}
                </CommandItem>
              )}
            </CommandGroup>
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
