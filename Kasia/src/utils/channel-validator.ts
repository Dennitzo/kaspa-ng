import { MAX_BROADCAST_CHANNEL_NAME } from "../config/constants";

export type ValidationResult = {
  isValid: boolean;
  error?: string;
};

// validate broadcast name is acceptable
export const validateChannelName = (name: string): ValidationResult => {
  const trimmed = name.trim();

  if (!trimmed) {
    return { isValid: false, error: "Channel name cannot be empty" };
  }

  if (trimmed.length > MAX_BROADCAST_CHANNEL_NAME) {
    return {
      isValid: false,
      error: `Channel name cannot exceed ${MAX_BROADCAST_CHANNEL_NAME} characters`,
    };
  }

  if (/\s/.test(trimmed)) {
    return { isValid: false, error: "Channel name cannot contain spaces" };
  }

  return { isValid: true };
};

// validates and normalizes a broadcast channel name for store operations
// throws an error if invalid, returns normalized name if valid.
export const validateAndNormalizeChannelName = (name: string): string => {
  const result = validateChannelName(name);
  if (!result.isValid) {
    throw new Error(result.error);
  }
  return name.trim().toLowerCase();
};
