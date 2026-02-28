import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase } from "../api/config";

const extractTransactionFromSearchResponse = (payload: unknown): TransactionData | null => {
  if (Array.isArray(payload)) {
    return payload.length > 0 ? (payload[0] as TransactionData) : null;
  }

  if (payload && typeof payload === "object") {
    const objectPayload = payload as Record<string, unknown>;
    const transactions = objectPayload.transactions;
    if (Array.isArray(transactions) && transactions.length > 0) {
      return transactions[0] as TransactionData;
    }
    const data = objectPayload.data;
    if (Array.isArray(data) && data.length > 0) {
      return data[0] as TransactionData;
    }
  }

  return null;
};

const searchTransactionById = async (transactionId: string): Promise<TransactionData | null> => {
  const requests: Array<{
    body: Record<string, string[]>;
    params?: Record<string, string>;
  }> = [
    {
      body: { transactionIds: [transactionId] },
      params: { fields: "", resolve_previous_outpoints: "light" },
    },
    {
      body: { transaction_ids: [transactionId] },
      params: { fields: "", resolve_previous_outpoints: "light" },
    },
    {
      body: { transactionIds: [transactionId] },
    },
    {
      body: { transaction_ids: [transactionId] },
    },
  ];

  for (const request of requests) {
    try {
      const { data } = await axios.post(`${getApiBase()}/transactions/search`, request.body, {
        params: request.params,
      });
      const transaction = extractTransactionFromSearchResponse(data);
      if (transaction) return transaction;
    } catch {
      // Try next fallback shape/params.
    }
  }

  return null;
};

export const useTransactionById = (transactionId: string) =>
  useQuery({
    queryKey: ["transaction", { transactionId }],
    queryFn: async () => {
      let initialError: unknown;
      try {
        const { data } = await axios.get(
          `${getApiBase()}/transactions/${transactionId}?resolve_previous_outpoints=light`,
        );
        return data as TransactionData;
      } catch (err) {
        initialError = err;
      }

      const searchResult = await searchTransactionById(transactionId);
      if (searchResult) {
        return searchResult;
      }

      if (axios.isAxiosError(initialError)) {
        const status = initialError.response?.status;
        const statusText = initialError.response?.statusText;
        throw new Error(status ? `API ${status}${statusText ? ` ${statusText}` : ""}` : initialError.message);
      }

      throw initialError ?? new Error("Transaction not found");
    },
    enabled: !!transactionId,
    retry: (failureCount, error) => {
      if (error instanceof Error && error.message.startsWith("API 429")) {
        return failureCount < 3;
      }
      return failureCount < 5;
    },
    retryDelay: (attempt, error) => {
      if (error instanceof Error && error.message.startsWith("API 429")) {
        return Math.min(10_000, 1000 * 2 ** attempt);
      }
      return 1000;
    },
    refetchOnWindowFocus: false,
    staleTime: 30_000,
    gcTime: 5 * 60_000,
  });

export interface TransactionData {
  subnetwork_id: string;
  transaction_id: string;
  hash: string;
  mass: string;
  payload: string;
  block_hash: string[];
  block_time: number;
  is_accepted: boolean;
  accepting_block_hash: string;
  accepting_block_blue_score: number;
  accepting_block_time: number;
  inputs: Array<{
    transaction_id: string;
    index: number;
    previous_outpoint_hash: string;
    previous_outpoint_index: string;
    previous_outpoint_resolved: {
      transaction_id: string;
      index: number;
      amount: number;
      script_public_key: string;
      script_public_key_address: string;
      script_public_key_type: string;
      accepting_block_hash: string;
    };
    previous_outpoint_address: string;
    previous_outpoint_amount: number;
    signature_script: string;
    sig_op_count: string;
  }> | null;
  outputs: Array<{
    transaction_id: string;
    index: number;
    amount: number;
    script_public_key: string;
    script_public_key_address: string;
    script_public_key_type: string;
    accepting_block_hash: string;
  }> | null;
}
