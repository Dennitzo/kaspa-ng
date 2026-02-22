const SAVED_ADDRESS_KEY_PREFIX = "kaspaExplorerSavedAddress";

const sanitizeNetworkId = (networkId: string) => {
  const value = networkId.trim().toLowerCase();
  return value.replace(/[^a-z0-9-]/g, "-") || "mainnet";
};

export const savedAddressKeyForNetwork = (networkId: string) =>
  `${SAVED_ADDRESS_KEY_PREFIX}:${sanitizeNetworkId(networkId)}`;
