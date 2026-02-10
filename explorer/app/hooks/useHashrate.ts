import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

interface HashrateResponse {
  hashrate: number;
}

export const useHashrate = () =>
  useQuery({
    queryKey: ["hashrate"],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl("info/hashrate"));
      return data as HashrateResponse;
    },
    refetchInterval: 20000,
    staleTime: Infinity,
  });
