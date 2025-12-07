interface BadgeProps {
  children: React.ReactNode;
  variant?: "default" | "outline";
}

export const Badge = ({ children, variant = "default" }: BadgeProps) => {
  const baseStyles = "inline-flex items-center gap-1.5 px-3 py-1 text-xs font-medium rounded-full";
  const variants = {
    default: "bg-primary/10 text-primary border border-primary/20",
    outline: "bg-transparent text-muted-foreground border border-border",
  };

  return <span className={`${baseStyles} ${variants[variant]}`}>{children}</span>;
};
