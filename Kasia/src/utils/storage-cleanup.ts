// cleans up legacy localStorage keys that stored wallet addresses (privacy leaking sorta stuff).
// this can be removed at some point in the future
// this was added in v6.3

function removeKeysByPattern(pattern: string): number {
  let removedCount = 0;

  for (let i = localStorage.length - 1; i >= 0; i--) {
    const key = localStorage.key(i);
    if (key && key.includes(pattern)) {
      localStorage.removeItem(key);
      removedCount++;
    }
  }

  return removedCount;
}

// cleans up legacy contact storage and migration keys
export function cleanupLegacyLocalStorage(): void {
  let totalCleaned = 0;

  // clean up migration storage keys
  totalCleaned += removeKeysByPattern("_migrate_storage_v2");

  // clean up legacy contact storage keys (remove all matching patterns)
  totalCleaned += removeKeysByPattern("kasia_last_opened_recipient_");
  totalCleaned += removeKeysByPattern("kasia_last_opened_contact_id_");
  totalCleaned += removeKeysByPattern("kasia_last_opened_channel_id_");

  // clean up legacy encrypted conversations and messages keys
  totalCleaned += removeKeysByPattern("encrypted_conversations_kaspa");
  totalCleaned += removeKeysByPattern("kaspa_messages_by_wallet");

  if (totalCleaned > 0) {
    console.log(`Cleaned up ${totalCleaned} legacy localStorage keys.`);
  }
}
