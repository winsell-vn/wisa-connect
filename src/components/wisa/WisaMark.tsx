import wisaLogo from "@/assets/brand/wisa-logo-badge.png";
import { cn } from "@/lib/utils";

/**
 * The WISA emblem on its navy tile, copied verbatim from the web app's
 * `public/branding/wisa-logo-badge.png` so the desktop icon and the site icon
 * stay the same file instead of a redraw. The tile is part of the asset, which
 * is what keeps the mark readable on both the light and the dark theme.
 */
export function WisaMark({ className }: { className?: string }) {
  return (
    <img
      src={wisaLogo}
      alt="WISA"
      draggable={false}
      className={cn("h-8 w-8 shrink-0 select-none", className)}
    />
  );
}
