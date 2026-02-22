import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

export const useTransactionsCount = () =>
  useQuery({
    queryKey: ["transactionsCount"],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/transactions/count/`);
      return data as TransactionCount;
    },
  });

interface TransactionCount {
  timestamp: number;
  dateTime: string;
  coinbase: number;
  regular: number;
}
