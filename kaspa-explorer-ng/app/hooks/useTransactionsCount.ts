import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase } from "../api/config";

export const useTransactionsCount = () =>
  useQuery({
    queryKey: ["transactionsCount"],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/transactions/count/`);
      return data as TransactionCount;
    },
  });

interface TransactionCount {
  timestamp: number;
  dateTime: string;
  coinbase: number;
  regular: number;
}
