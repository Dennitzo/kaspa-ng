import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl, VPROGS_BASE } from "../api/urls";

export const useTopAddresses = () =>
  useQuery({
    queryKey: ["topAddresses"],
    queryFn: async () => {
      const vprogsBase = VPROGS_BASE ? VPROGS_BASE.replace(/\/$/, "") : "";
      const vprogsUrl = vprogsBase ? `${vprogsBase}/api/addresses/top` : "";
      if (vprogsUrl) {
        try {
          const { data } = await axios.get(vprogsUrl);
          return data[0] as TopAddresses;
        } catch {
          // fall back to kaspa-rest-server
        }
      }
      const { data } = await axios.get(apiUrl("addresses/top"));
      return data[0] as TopAddresses;
    },
    refetchInterval: 60000,
  });

interface TopAddresses {
  timestamp: number;
  hash?: string;
  ranking: {
    rank: number;
    address: string;
    amount: number;
  }[];
}
