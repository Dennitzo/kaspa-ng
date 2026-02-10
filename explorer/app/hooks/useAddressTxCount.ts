import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

export const useAddressTxCount = (address: string) =>
  useQuery({
    queryKey: ["txCount", { address }],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl(`addresses/${address}/transactions-count`));
      return data as TxCount;
    },
    refetchInterval: 60000,
  });

interface TxCount {
  total: number;
  accepted?: number;
  limit_exceeded: boolean;
}
