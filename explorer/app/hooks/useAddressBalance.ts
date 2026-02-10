import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

export const useAddressBalance = (address: string) =>
  useQuery({
    queryKey: ["addresses", { address }],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl(`addresses/${address}/balance`));
      return data as AddressBalance;
    },
    refetchInterval: 60000,
  });

export interface AddressBalance {
  address: string;
  balance: number;
}
