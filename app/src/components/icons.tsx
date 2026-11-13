// Inline icons (stroke = currentColor). All decorative: aria-hidden.

interface P {
  size?: number;
}

const base = (size: number) => ({ width: size, height: size, viewBox: "0 0 24 24", fill: "none", stroke: "currentColor", strokeLinecap: "round" as const, strokeLinejoin: "round" as const, "aria-hidden": true });

export const Logo = ({ size = 18 }: P) => (
  <svg width={size} height={size} viewBox="0 0 18 18" aria-hidden="true">
    <circle cx="3" cy="11" r="1.6" fill="#FF6A2B" />
    <circle cx="7" cy="7" r="1.6" fill="#E0389A" />
    <circle cx="7" cy="11" r="1.6" fill="#FF6A2B" />
    <circle cx="11" cy="3" r="1.6" fill="#6B4BD8" />
    <circle cx="11" cy="7" r="1.6" fill="#E0389A" />
    <circle cx="11" cy="11" r="1.6" fill="#FF6A2B" />
    <circle cx="15" cy="7" r="1.6" fill="#E0389A" />
    <circle cx="15" cy="11" r="1.6" fill="#FF6A2B" />
    <circle cx="11" cy="15" r="1.6" fill="#FF6A2B" />
  </svg>
);

export const HistoryIcon = ({ size = 14 }: P) => (
  <svg {...base(size)} strokeWidth={2}>
    <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
    <path d="M3 3v5h5M12 7v5l3 2" />
  </svg>
);
export const Chevron = ({ size = 12 }: P) => (
  <svg {...base(size)} strokeWidth={2.4}>
    <path d="M6 9l6 6 6-6" />
  </svg>
);
export const Sliders = ({ size = 16 }: P) => (
  <svg {...base(size)} strokeWidth={2}>
    <path d="M4 6h9M17 6h3M4 12h3M11 12h9M4 18h11M19 18h1" />
    <circle cx="15" cy="6" r="2" />
    <circle cx="9" cy="12" r="2" />
    <circle cx="17" cy="18" r="2" />
  </svg>
);
export const Arrow = ({ size = 16 }: P) => (
  <svg {...base(size)} strokeWidth={2.2}>
    <path d="M7 17L17 7M9 7h8v8" />
  </svg>
);
export const Play = ({ size = 12 }: P) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path d="M7 4.5v15l13-7.5z" />
  </svg>
);
export const Pause = ({ size = 12 }: P) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <rect x="6" y="4.5" width="4" height="15" rx="1" />
    <rect x="14" y="4.5" width="4" height="15" rx="1" />
  </svg>
);
export const Stop = ({ size = 12 }: P) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <rect x="6" y="6" width="12" height="12" rx="2" />
  </svg>
);
export const Download = ({ size = 14 }: P) => (
  <svg {...base(size)} strokeWidth={2}>
    <path d="M12 4v11M7 10l5 5 5-5M5 20h14" />
  </svg>
);
export const Search = ({ size = 14 }: P) => (
  <svg {...base(size)} strokeWidth={2}>
    <circle cx="11" cy="11" r="7" />
    <path d="M20 20l-4-4" />
  </svg>
);
export const Close = ({ size = 14 }: P) => (
  <svg {...base(size)} strokeWidth={2}>
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
);
export const Star = ({ size = 14, filled = false }: P & { filled?: boolean }) => (
  <svg {...base(size)} strokeWidth={1.8} fill={filled ? "currentColor" : "none"}>
    <path d="M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z" />
  </svg>
);
export const Trash = ({ size = 14 }: P) => (
  <svg {...base(size)} strokeWidth={1.8}>
    <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12h10l1-12M9 7V4h6v3" />
  </svg>
);
export const Reopen = ({ size = 14 }: P) => (
  <svg {...base(size)} strokeWidth={1.8}>
    <path d="M4 20h16M7 16l9.5-9.5a2.1 2.1 0 0 1 3 3L10 19H7z" />
  </svg>
);
export const Gear = ({ size = 14 }: P) => (
  <svg {...base(size)} strokeWidth={1.8}>
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3h0a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8v0a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" />
  </svg>
);
