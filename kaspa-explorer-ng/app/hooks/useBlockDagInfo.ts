import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

interface BlockdagInfo {
  networkName: string;
  blockCount: string;
  headerCount: string;
  tipHashes: string[];
  difficulty: number;
  pastMedianTime: string;
  virtualParentHashes: string[];
  pruningPointHash: string[];
  virtualDaaScore: string;
}

export const useBlockdagInfo = () =>
  useQuery({
    queryKey: ["blockdagInfo"],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/info/blockdag`);
      return data as BlockdagInfo;
    },
    refetchInterval: 20000,
    staleTime: Infinity,
  });
