const ADDRESS_BODY_REGEX = /^[qpzry9x8gf2tvdw0s3jn54khce6mua7l]{61,63}$/;
const ADDRESS_WITH_PREFIX_REGEX = /^(kaspa|kaspatest|kaspasim|kaspadev):[qpzry9x8gf2tvdw0s3jn54khce6mua7l]{61,63}$/;

const networkIdToAddressPrefix = (networkId: string) => {
  if (networkId.startsWith("testnet")) return "kaspatest";
  if (networkId.startsWith("simnet")) return "kaspasim";
  if (networkId.startsWith("devnet")) return "kaspadev";
  return "kaspa";
};

export const normalizeKaspaAddress = (address: string, networkId: string = "mainnet") => {
  const normalized = address.trim().toLowerCase();
  if (!normalized) return "";
  if (normalized.includes(":")) return normalized;
  if (!ADDRESS_BODY_REGEX.test(normalized)) return normalized;
  return `${networkIdToAddressPrefix(networkId)}:${normalized}`;
};

export const isValidKaspaAddressSyntax = (address: string, networkId: string = "mainnet") =>
  ADDRESS_WITH_PREFIX_REGEX.test(normalizeKaspaAddress(address, networkId));

export const isValidHashSyntax = (hash: string) => /^[0-9a-fA-F]{64}$/.test(hash);
