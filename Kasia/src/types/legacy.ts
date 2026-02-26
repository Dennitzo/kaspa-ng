export type LegacyMessage = {
  transactionId: string;
  senderAddress: string;
  recipientAddress: string;
  timestamp: number;
  content: string;
  amount: number;
  fee?: number;
  payload: string;
  fileData?: {
    type: string;
    name: string;
    size: number;
    mimeType: string;
    content: string;
  };
};

export type LegacyContact = {
  address: string;
  lastMessage: LegacyMessage;
  messages: LegacyMessage[];
  status?: "active" | "pending" | "rejected";
  nickname?: string;
};
