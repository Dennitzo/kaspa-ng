import { expect, it } from "vitest";
import { parseKaspaMessagePayload } from "../utils/message-payload";

it("should parse handshake", async () => {
  const message =
    "636970685f6d73673a313a68616e647368616b653aabebab0fbd2af36c3e388d84022a8689a0d2258b901529bd209e3c3a8e24050fee9a206e3bcc1bca527f4362bcc4f48461b621b58445be2d2ab62fdd15bb8ce291041a773aacabf4b21b72e6b4a15499ccc04f11adc6fe17ac10acc5d4bdea8bd8ecd55e9b5b4600eb5a4bfa5195a906c48871c7c1c7fd47c62be3fa88f01c86c37f192546ab75961c990640a2af";
  const encryptedHex =
    "abebab0fbd2af36c3e388d84022a8689a0d2258b901529bd209e3c3a8e24050fee9a206e3bcc1bca527f4362bcc4f48461b621b58445be2d2ab62fdd15bb8ce291041a773aacabf4b21b72e6b4a15499ccc04f11adc6fe17ac10acc5d4bdea8bd8ecd55e9b5b4600eb5a4bfa5195a906c48871c7c1c7fd47c62be3fa88f01c86c37f192546ab75961c990640a2af";
  const parsed = parseKaspaMessagePayload(message);

  expect(parsed.encryptedHex).toBe(encryptedHex);
  expect(parsed.type).toBe("handshake");
  expect(parsed.alias).toBe(undefined);
});

it("should parse message", async () => {
  const message =
    "636970685f6d73673a313a636f6d6d3a3230663932363065623738313a583731357435574670537a6742515a6341727375706c5a6d64324768682f35307a6d786a41737347705839454e782b5148664d394454784b506d53506666496458624e715741544753754f7365776c57626167486b42593d";
  const expectedAlias = "20f9260eb781";
  const encryptedHex =
    "583731357435574670537a6742515a6341727375706c5a6d64324768682f35307a6d786a41737347705839454e782b5148664d394454784b506d53506666496458624e715741544753754f7365776c57626167486b42593d";

  const parsed = parseKaspaMessagePayload(message);

  expect(parsed.encryptedHex).toBe(encryptedHex);
  expect(parsed.type).toBe("comm");
  expect(parsed.alias).toBe(expectedAlias);
});

it("should parse payment", async () => {
  const message =
    "636970685f6d73673a313a7061796d656e743a7b2274797065223a227061796d656e74222c226d657373616765223a2268656c6c6f20776f726c64222c22616d6f756e74223a302e35323334353637382c2274696d657374616d70223a313735353930373739333033322c2276657273696f6e223a317d";
  const encryptedHex =
    "7b2274797065223a227061796d656e74222c226d657373616765223a2268656c6c6f20776f726c64222c22616d6f756e74223a302e35323334353637382c2274696d657374616d70223a313735353930373739333033322c2276657273696f6e223a317d";

  const parsed = parseKaspaMessagePayload(message);

  expect(parsed.encryptedHex).toBe(encryptedHex);
  expect(parsed.type).toBe("payment");
  expect(parsed.alias).toBe(undefined);
});
