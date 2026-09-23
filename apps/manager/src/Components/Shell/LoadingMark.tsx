/** The small MXBsecure mark used while a full page is changing state. */
export const LOADING_MARK_GLYPH = "m";

export function LoadingMark({
  className,
  label = "Loading",
}: {
  className?: string;
  label?: string;
}) {
  return (
    <span role="status" aria-label={label} className={className}>
      <span
        aria-hidden="true"
        className="inline-block animate-pulse font-cond text-[29px] font-extrabold leading-none tracking-[-0.06em] motion-reduce:animate-none"
      >
        {LOADING_MARK_GLYPH}
      </span>
    </span>
  );
}
