import { PublicKey } from "wasm/kaspa";

export type Wallet = {
  id: string;
  name: string;
  createdAt: string;
};

export type UnlockedWallet = {
  id: string;
  name: string;
  activeAccount: 1;
  encryptedPrivateKey: string;
  password: string;
  receivePublicKey: PublicKey;
  passphrase?: string;
};

export type StoredWallet = {
  id: string;
  name: string;
  encryptedPhrase: string;
  createdAt: string;
  accounts: { name: string }[];
  encryptedPassphrase?: string;
};

export type WalletBalance = {
  mature: bigint;
  pending: bigint;
  outgoing: bigint;
  matureDisplay: string;
  pendingDisplay: string;
  outgoingDisplay: string;
  matureUtxoCount: number;
  pendingUtxoCount: number;
} | null;
