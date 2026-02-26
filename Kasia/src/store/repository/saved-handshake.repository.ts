import { DBNotFoundException, KasiaDB } from "./db";

export type DbSavedHandshake = {
  /**
   * `${tenantId}_${transactionId}`
   */
  id: string;

  tenantId: string;

  createdAt: Date;
};

export type SavedHandshake = DbSavedHandshake;

export class SavedHandhshakeRepository {
  constructor(
    readonly db: KasiaDB,
    readonly tenantId: string
  ) {}

  async getSavedHandshake(id: string): Promise<SavedHandshake> {
    const result = await this.db.get("savedHandshakes", id);

    if (!result) {
      throw new DBNotFoundException();
    }

    return result;
  }

  async saveSavedHandshake(
    savedHandhshake: Omit<SavedHandshake, "tenantId">
  ): Promise<void> {
    await this.db.put("savedHandshakes", {
      id: savedHandhshake.id,
      createdAt: savedHandhshake.createdAt,
      tenantId: this.tenantId,
    });
    return;
  }

  /**
   * returns null in case there is no saved handshakes
   */
  async getLastSavedHandshakesByCreatedAt(): Promise<SavedHandshake | null> {
    const cursor = await this.db
      .transaction("savedHandshakes", "readonly")
      .objectStore("savedHandshakes")
      .index("by-tenant-id-created-at")
      .openCursor(IDBKeyRange.upperBound([this.tenantId, new Date()]), "prev");

    if (!cursor) {
      return null;
    }

    return cursor.value;
  }

  async getAllSavedHandshakes(): Promise<SavedHandshake[]> {
    return this.db.getAllFromIndex(
      "savedHandshakes",
      "by-tenant-id",
      this.tenantId
    );
  }

  async doesExistsById(savedHanshakeId: string): Promise<boolean> {
    return this.db
      .count("savedHandshakes", savedHanshakeId)
      .then((count) => count > 0);
  }

  async deleteTenant(tenantId: string): Promise<void> {
    const keys = await this.db.getAllKeysFromIndex(
      "savedHandshakes",
      "by-tenant-id",
      tenantId
    );

    await Promise.all(keys.map((k) => this.db.delete("savedHandshakes", k)));
  }

  async saveBulk(
    savedHandshakes: Omit<SavedHandshake, "tenantId">[]
  ): Promise<void> {
    const tx = this.db.transaction("savedHandshakes", "readwrite");
    const store = tx.objectStore("savedHandshakes");

    for (const handshake of savedHandshakes) {
      // Check if handshake already exists
      const existing = await store.get(handshake.id);
      if (!existing) {
        await store.put({
          id: handshake.id,
          createdAt: handshake.createdAt,
          tenantId: this.tenantId,
        });
      }
    }

    await tx.done;
  }
}
