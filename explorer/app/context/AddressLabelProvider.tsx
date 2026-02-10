import { createContext, useEffect, useState } from "react";
import { VPROGS_BASE } from "../api/urls";

export const AddressLabelContext = createContext<{ labels: Record<string, string> }>({ labels: {} });

const REFRESH_INTERVAL_MS = 60000;

export const AddressLabelProvider = ({ children }: { children: React.ReactNode }) => {
  const [labels, setLabels] = useState<Record<string, string>>({});

  useEffect(() => {
    if (typeof window === "undefined") return;
    const baseUrl = (VPROGS_BASE || `http://${window.location.hostname}:19115`).replace(/\/$/, "");

    const loadLabels = async () => {
      try {
        const response = await fetch(`${baseUrl}/api/address-labels`, { cache: "no-store" });
        if (!response.ok) return;
        const payload = await response.json();
        if (payload?.labels) {
          setLabels(payload.labels);
        }
      } catch {
        // ignore fetch errors
      }
    };

    loadLabels();
    const interval = window.setInterval(loadLabels, REFRESH_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, []);

  return <AddressLabelContext.Provider value={{ labels }}>{children}</AddressLabelContext.Provider>;
};
