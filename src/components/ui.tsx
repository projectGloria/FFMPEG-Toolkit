import React from "react";

// Tiny design-system layer so every page shares one look instead of
// re-typing 200-char className strings (the old pages each had their own).

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

const VARIANTS: Record<ButtonVariant, string> = {
  primary: "bg-purple-600 hover:bg-purple-500 text-white",
  secondary: "bg-zinc-800 hover:bg-zinc-700 text-zinc-100 border border-zinc-700",
  ghost: "text-zinc-400 hover:text-zinc-100 hover:bg-zinc-800",
  danger: "text-rose-400 hover:text-rose-300 hover:bg-rose-500/10",
};

export const Button: React.FC<
  React.ButtonHTMLAttributes<HTMLButtonElement> & {
    variant?: ButtonVariant;
    icon?: React.ReactNode;
    size?: "sm" | "md";
  }
> = ({ variant = "secondary", icon, size = "md", className = "", children, ...rest }) => (
  <button
    {...rest}
    className={`inline-flex items-center justify-center gap-1.5 rounded-md font-medium transition-colors disabled:opacity-40 disabled:pointer-events-none ${
      size === "sm" ? "h-7 px-2 text-xs" : "h-8 px-3 text-[13px]"
    } ${VARIANTS[variant]} ${className}`}
  >
    {icon}
    {children}
  </button>
);

export const IconButton: React.FC<
  React.ButtonHTMLAttributes<HTMLButtonElement> & { label: string; danger?: boolean }
> = ({ label, danger, className = "", children, ...rest }) => (
  <button
    {...rest}
    title={label}
    aria-label={label}
    className={`inline-flex h-7 w-7 items-center justify-center rounded-md transition-colors disabled:opacity-30 disabled:pointer-events-none ${
      danger
        ? "text-zinc-500 hover:text-rose-400 hover:bg-rose-500/10"
        : "text-zinc-500 hover:text-zinc-100 hover:bg-zinc-800"
    } ${className}`}
  >
    {children}
  </button>
);

export const Toggle: React.FC<{
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label?: string;
}> = ({ checked, onChange, disabled, label }) => (
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    disabled={disabled}
    onClick={() => onChange(!checked)}
    className={`relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors disabled:opacity-40 ${
      checked ? "bg-purple-600" : "bg-zinc-700"
    }`}
  >
    <span
      className={`inline-block h-4 w-4 rounded-full bg-white shadow transition-transform ${
        checked ? "translate-x-[18px]" : "translate-x-0.5"
      }`}
    />
  </button>
);

export const PageHeader: React.FC<{
  title: string;
  subtitle?: React.ReactNode;
  actions?: React.ReactNode;
}> = ({ title, subtitle, actions }) => (
  <div className="flex items-end justify-between gap-4 border-b border-zinc-800 px-6 pb-4 pt-5">
    <div className="min-w-0">
      <h1 className="text-lg font-semibold text-zinc-50">{title}</h1>
      {subtitle && <p className="mt-0.5 text-[13px] text-zinc-400">{subtitle}</p>}
    </div>
    {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
  </div>
);

export const Section: React.FC<{
  title: string;
  description?: React.ReactNode;
  children: React.ReactNode;
  actions?: React.ReactNode;
}> = ({ title, description, children, actions }) => (
  <section className="rounded-lg border border-zinc-800 bg-zinc-900/60">
    <div className="flex items-start justify-between gap-4 border-b border-zinc-800 px-4 py-3">
      <div>
        <h2 className="text-sm font-semibold text-zinc-100">{title}</h2>
        {description && <p className="mt-0.5 text-xs text-zinc-400">{description}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
    <div className="divide-y divide-zinc-800">{children}</div>
  </section>
);

/** One label/description + control row inside a Section. */
export const Row: React.FC<{
  label: string;
  description?: React.ReactNode;
  children: React.ReactNode;
}> = ({ label, description, children }) => (
  <div className="flex items-center justify-between gap-6 px-4 py-3">
    <div className="min-w-0">
      <div className="text-[13px] font-medium text-zinc-200">{label}</div>
      {description && <div className="mt-0.5 text-xs text-zinc-500">{description}</div>}
    </div>
    <div className="shrink-0">{children}</div>
  </div>
);

export const ProgressBar: React.FC<{
  value: number;
  indeterminate?: boolean;
  tone?: "accent" | "success" | "danger" | "muted";
}> = ({ value, indeterminate, tone = "accent" }) => {
  const color = {
    accent: "bg-purple-500",
    success: "bg-emerald-500",
    danger: "bg-rose-500",
    muted: "bg-zinc-600",
  }[tone];
  return (
    <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800">
      {indeterminate ? (
        <div className={`h-full w-1/3 animate-indeterminate rounded-full ${color}`} />
      ) : (
        <div
          className={`h-full rounded-full transition-[width] duration-200 ${color}`}
          style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
        />
      )}
    </div>
  );
};

export const inputClass =
  "w-full rounded-md border border-zinc-700 bg-zinc-950 px-2.5 py-1.5 text-[13px] text-zinc-100 placeholder:text-zinc-600 focus:border-purple-500 focus:outline-none focus:ring-1 focus:ring-purple-500";
