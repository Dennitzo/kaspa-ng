import { core } from "@tauri-apps/api";
import { create } from "zustand";
import {
  hasData,
  setData,
  getData,
  checkStatus,
} from "@tauri-apps/plugin-biometry";
import { platform } from "@tauri-apps/plugin-os";

type SessionState = {
  supportSecuredBiometry: () => Promise<boolean>;

  /**
   * Can be called on any plateform, no-op is secured biometry isn't supported
   */
  setSession: (tenantId: string, password: string) => Promise<void>;
  /**
   * Can be called on any plateform, return false if secured biometry isn't supported
   */
  hasSession: (tenantId: string) => Promise<boolean>;

  /**
   * Prompt an authentication prior getting the session
   * You must call `hasSession` to check if a session exists first
   *
   * @returns the stored password
   */
  getSession: (tenantId: string) => Promise<string | null>;
};

export const useSessionState = create<SessionState>((set, get) => {
  return {
    async supportSecuredBiometry() {
      return false;
    },
    async getSession(_tenantId) {
      return null;
    },
    async hasSession(_tenantId) {
      return false;
    },
    async setSession(_tenantId, _password) {
      return;
    },
  };
});
