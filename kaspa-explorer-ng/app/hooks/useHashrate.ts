import { useQuery } from "@tanstack/react-query";
import axios from "axios";

interface HashrateInfo {
  hashrate: number;
}

export const useHashrate = () =>
  useQuery({
    queryKey: ["hashrate"],
    queryFn: async () => {
      const { data } = await axios.get("https://api-tn12.kaspa.org/info/hashrate");
      return data as HashrateInfo;
    },
    refetchInterval: 20000,
  });
