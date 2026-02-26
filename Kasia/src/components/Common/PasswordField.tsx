import clsx from "clsx";
import { ChangeEventHandler, forwardRef, useState, type FC } from "react";
import { Eye, EyeOff } from "lucide-react";

export interface PasswordFieldProps {
  /**
   * default is "password"
   */
  id?: string;
  /**
   * default is "password"
   */
  name?: string;
  /**
   * default is "Enter your password"
   */
  placeholder?: string;
  value?: string;
  onChange?: ChangeEventHandler<HTMLInputElement>;
  hasError?: boolean;
  disabled?: boolean;
  required?: boolean;
  /**
   * If not present, no label added to DOM
   */
  label?: string;

  classInput?: string;
  classLabel?: string;
}

export const PasswordField = forwardRef<HTMLInputElement, PasswordFieldProps>(
  (props, ref) => {
    const [showPassword, setShowPassword] = useState(false);

    const togglePasswordVisibility = () => {
      setShowPassword((prev) => !prev);
    };

    return (
      <>
        {props.label ? (
          <label
            htmlFor={props.id ?? "password"}
            className={clsx(props.classLabel)}
          >
            {props.label}
          </label>
        ) : null}
        <div className="relative">
          <input
            id={props.id ?? "password"}
            name={props.name ?? "password"}
            autoComplete="current-password"
            ref={ref}
            type={showPassword ? "text" : "password"}
            placeholder={props.placeholder ?? "Enter your password"}
            onChange={props.onChange}
            className={clsx(props.classInput, "pr-10", {
              "!border-red-500": props.hasError,
            })}
            disabled={props.disabled}
            required={props.required}
            value={props.value}
          />
          <button
            type="button"
            onClick={togglePasswordVisibility}
            className="absolute inset-y-0 right-0 flex items-center pr-3"
            aria-label={showPassword ? "Hide password" : "Show password"}
          >
            {showPassword ? (
              <EyeOff className="h-5 w-5 text-gray-400" />
            ) : (
              <Eye className="h-5 w-5 text-gray-400" />
            )}
          </button>
        </div>
      </>
    );
  }
);
