import { KasiaDB, DBNotFoundException } from "./db";

export type DbDecryptionTrial = {
  /**
   * `${tenantId}_${txId}`
   * @note tenantId is automatically added upon saving
   */
  id: string;

  tenantId: string;
  timestamp: Date;
};

export type DecryptionTrial = DbDecryptionTrial;

export class DecryptionTrialRepository {
  constructor(
    readonly db: KasiaDB,
    readonly tenantId: string,
    readonly walletPassword: string
  ) {}

  async getDecryptionTrial(
    decryptionTrialId: string
  ): Promise<DecryptionTrial> {
    const result = await this.db.get("decryptionTrials", decryptionTrialId);

    if (!result) {
      throw new DBNotFoundException();
    }

    return result;
  }

  async getDecryptionTrials(): Promise<DecryptionTrial[]> {
    return this.db.getAllFromIndex(
      "decryptionTrials",
      "by-tenant-id",
      this.tenantId
    );
  }

  /**
   * @param decryptionTrial id of the transaction to mark as failed, `tenantId` is automatically added
   */
  async saveDecryptionTrial(
    decryptionTrial: Omit<DecryptionTrial, "tenantId">
  ): Promise<string> {
    return this.db.put("decryptionTrials", {
      id: `${this.tenantId}_${decryptionTrial.id}`,
      tenantId: this.tenantId,
      timestamp: decryptionTrial.timestamp,
    });
  }

  async deleteDecryptionTrial(decryptionTrialId: string): Promise<void> {
    await this.db.delete("decryptionTrials", decryptionTrialId);
    return;
  }

  async deleteTenant(tenantId: string): Promise<void> {
    const keys = await this.db.getAllKeysFromIndex(
      "decryptionTrials",
      "by-tenant-id",
      tenantId
    );

    await Promise.all(keys.map((k) => this.db.delete("decryptionTrials", k)));
  }

  async saveBulk(
    decryptionTrials: Omit<DecryptionTrial, "tenantId">[]
  ): Promise<void> {
    const tx = this.db.transaction("decryptionTrials", "readwrite");
    const store = tx.objectStore("decryptionTrials");

    for (const trial of decryptionTrials) {
      // Check if trial already exists
      const existing = await store.get(`${this.tenantId}_${trial.id}`);
      if (!existing) {
        await store.put({
          id: `${this.tenantId}_${trial.id}`,
          tenantId: this.tenantId,
          timestamp: trial.timestamp,
        });
      }

      await tx.done;
    }
  }
}
