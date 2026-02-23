import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase } from "../api/config";

export const useAddressDistribution = () =>
  useQuery({
    queryKey: ["addressDistribution"],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/addresses/distribution`);
      return data as AddressDistribution[];
    },
  });

export interface AddressDistribution {
  tiers: {
    tier: number;
    count: number;
    amount: number;
  }[];
  timestamp: number;
}
