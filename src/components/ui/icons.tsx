import type { ReactNode, SVGProps } from "react";

type IconProps = Omit<SVGProps<SVGSVGElement>, "children"> & { size?: number };

function Base({ size = 16, children, ...rest }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {children}
    </svg>
  );
}

export function FolderIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M3 7a2 2 0 0 1 2-2h3.6l2 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </Base>
  );
}

export function SearchIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="11" cy="11" r="7" />
      <path d="M16.5 16.5 21 21" />
    </Base>
  );
}

export function BookIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M4 5a2 2 0 0 1 2-2h13v16H6a2 2 0 0 0-2 2z" />
      <path d="M4 19a2 2 0 0 1 2-2h13" />
    </Base>
  );
}

export function CalendarIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="5" width="18" height="16" rx="2" />
      <path d="M3 10h18M8 3v4M16 3v4" />
    </Base>
  );
}

export function ActivityIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M3 12h3.5l2.5-6 3.5 12 2.5-6H21" />
    </Base>
  );
}

export function SettingsIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M4 8h10M18 8h2M4 16h2M10 16h10" />
      <circle cx="16" cy="8" r="2" />
      <circle cx="8" cy="16" r="2" />
    </Base>
  );
}

export function PlusIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 5v14M5 12h14" />
    </Base>
  );
}

export function ChevronLeftIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M15 6l-6 6 6 6" />
    </Base>
  );
}

export function ChevronRightIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M9 6l6 6-6 6" />
    </Base>
  );
}

export function TrashIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M4 7h16" />
      <path d="M9 7V5h6v2" />
      <path d="M6.5 7l1 13h9l1-13" />
      <path d="M10.5 11v6M13.5 11v6" />
    </Base>
  );
}

export function PanelLeftIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M9 4v16" />
    </Base>
  );
}

export function DocIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v4h4" />
    </Base>
  );
}

export function PencilIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M4 20h4l10.5-10.5a2.1 2.1 0 0 0-3-3L5 17z" />
      <path d="M13.5 6.5l4 4" />
    </Base>
  );
}

export function CopyIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="9" y="9" width="12" height="12" rx="2" />
      <path d="M15 9V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h4" />
    </Base>
  );
}

export function FolderPlusIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M3 7a2 2 0 0 1 2-2h3.6l2 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      <path d="M12 11v5M9.5 13.5h5" />
    </Base>
  );
}

export function FilePlusIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v4h4" />
      <path d="M12 11v5M9.5 13.5h5" />
    </Base>
  );
}

export function GlobeIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18" />
      <path d="M12 3a15 15 0 0 1 0 18a15 15 0 0 1 0-18z" />
    </Base>
  );
}

export function ShieldIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 3l7 3v6c0 4.5-3 7.6-7 9-4-1.4-7-4.5-7-9V6z" />
    </Base>
  );
}

export function SparkIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 3l1.8 4.6L18 9.4l-4.2 1.8L12 15.8l-1.8-4.6L6 9.4l4.2-1.8z" />
      <path d="M18.5 16.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z" />
    </Base>
  );
}

export function StopIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="7" y="7" width="10" height="10" rx="2" />
    </Base>
  );
}

export function ArrowUpIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M12 19V5M6 11l6-6 6 6" />
    </Base>
  );
}

export function SunIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </Base>
  );
}

export function MoonIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M20 14.5A8.5 8.5 0 0 1 9.5 4a8.5 8.5 0 1 0 10.5 10.5z" />
    </Base>
  );
}

export function MonitorIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M9 20h6M12 16v4" />
    </Base>
  );
}

export function PaletteIcon(props: IconProps) {
  return (
    <Base {...props}>
      <circle cx="13.5" cy="6.5" r="5.5" />
      <circle cx="18.5" cy="10.5" r="1.5" />
      <circle cx="7.5" cy="7.5" r="1.5" />
      <circle cx="10.5" cy="18.5" r="1.5" />
      <circle cx="18.5" cy="19.5" r="1.5" />
      <circle cx="6.5" cy="17.5" r="1.5" />
    </Base>
  );
}

export function CpuIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <path d="M4 8h4M4 12h4M4 16h4M16 8h4M16 12h4M16 16h4M8 4v4M12 4v4M8 20v4M12 20v4" />
    </Base>
  );
}

export function DensityIcon(props: IconProps) {
  return (
    <Base {...props}>
      <rect x="3" y="3" width="6" height="6" rx="1" />
      <rect x="15" y="3" width="6" height="6" rx="1" />
      <rect x="3" y="15" width="6" height="6" rx="1" />
      <rect x="15" y="15" width="6" height="6" rx="1" />
      <rect x="9" y="9" width="6" height="6" rx="1" />
    </Base>
  );
}

export function XIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M18 6 6 18M6 6l12 12" />
    </Base>
  );
}

export function HomeIcon(props: IconProps) {
  return (
    <Base {...props}>
      <path d="M3 9.5l9-7 9 7v10.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 20V9.5z" />
      <path d="M9 21v-6a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v6" />
    </Base>
  );
}

