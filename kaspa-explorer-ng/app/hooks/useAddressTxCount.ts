import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase } from "../api/config";

export const useAddressTxCount = (address: string) =>
  useQuery({
    queryKey: ["txCount", { address }],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/addresses/${address}/transactions-count`);
      return data as TxCount;
    },
  });

interface TxCount {
  total: number;
  limit_exceeded: boolean;
}
