import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

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
      const { data } = await axios.get(apiUrl("info/blockdag"));
      return data as BlockdagInfo;
    },
    refetchInterval: 20000,
    staleTime: Infinity,
  });
