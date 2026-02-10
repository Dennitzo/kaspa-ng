import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

export const useBlockByBlueScore = (blueScore: string) =>
  useQuery({
    queryKey: ["block", { blueScore }],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl("blocks-from-bluescore"), {
        params: {
          blueScore: Number(blueScore),
          includeTransactions: false,
        },
      });
      return data as BlockByBlueScore[];
    },
    enabled: !!blueScore,
    retry: false,
  });

interface BlockByBlueScore {
  verboseData?: {
    hash?: string;
  };
}
