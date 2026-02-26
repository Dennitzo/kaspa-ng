import { Contact } from "../store/repository/contact.repository";
import { Conversation } from "../store/repository/conversation.repository";

export interface HandshakePayload {
  type: "handshake";
  /**
   * my alias, the one I write to
   */
  alias: string;
  theirAlias?: string; // Used in response to confirm both aliases
  timestamp: number;
  version: number; // for future protocol upgrades
  recipientAddress?: string; // Only used for initial handshake
  sendToRecipient?: boolean; // Flag to indicate if message should be sent to recipient
  isResponse?: boolean; // Flag to indicate this is a response
}

export type SavedHandshakePayload = HandshakePayload & {
  recipientAddress: string;
};

export interface PaymentPayload {
  type: "payment";
  message: string;
  amount: number; // Amount in KAS
  timestamp: number;
  version: number;
}

export interface ConversationEvents {
  onHandshakeInitiated: (conversation: Conversation, contact: Contact) => void;
  onHandshakeCompleted: (conversation: Conversation, contact: Contact) => void;
  onHandshakeExpired: (conversation: Conversation, contact: Contact) => void;
  onError: (error: unknown) => void;
}
