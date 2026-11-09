import { cn } from "@/lib/utils";

/**
 * The WISA mark: an orange tile with a "W" cut out of it. Deliberately a plain
 * SVG so it inherits the surrounding text colour and needs no asset pipeline.
 */
export function WisaMark({ className }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 32 32"
      role="img"
      aria-label="WISA"
      className={cn("h-8 w-8", className)}
    >
      <defs>
        <linearGradient id="wisa-mark" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="#f97316" />
          <stop offset="100%" stopColor="#ea580c" />
        </linearGradient>
      </defs>
      <rect width="32" height="32" rx="9" fill="url(#wisa-mark)" />
      <path
        d="M7 11.5 10.6 21h2.9l2.5-6.4 2.5 6.4h2.9L25 11.5h-3.1l-1.9 6.1-2.4-6.1h-2.2l-2.4 6.1-1.9-6.1Z"
        fill="#fff"
      />
    </svg>
  );
}
