import type { ButtonHTMLAttributes, ReactNode } from "react";
import "./Button.css";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "danger" | "ghost";
  iconLeft?: ReactNode;
  iconRight?: ReactNode;
  children: ReactNode;
}

export function Button({
  variant = "primary",
  iconLeft,
  iconRight,
  children,
  className = "",
  ...props
}: ButtonProps) {
  return (
    <button className={`common-btn btn-${variant} ${className}`} {...props}>
      {iconLeft && <span className="btn-icon">{iconLeft}</span>}
      <span className="btn-text">{children}</span>
      {iconRight && <span className="btn-icon">{iconRight}</span>}
    </button>
  );
}
