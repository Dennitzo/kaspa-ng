import { encryptXChaCha20Poly1305, decryptXChaCha20Poly1305 } from "kaspa-wasm";
import { TransactionId } from "../../types/transactions";
import { KasiaDB, DBNotFoundException } from "./db";

export type DbHandshake = {
  /**
   * `${tenantId}_${transactionId}`
   */
  id: string;
  /**
   * tenant is the selected wallet
   */
  tenantId: string;
  conversationId: string;
  createdAt: Date;
  transactionId: TransactionId;
  contactId: string;
  /**
   * encrypted data shaped as `json(HandshakeBag)`
   */
  encryptedData: string;
};

export type HandshakeBag = {
  amount: number;
  fee?: number;
  content: string;
  fromMe: boolean;
};
export type Handshake = HandshakeBag &
  Omit<DbHandshake, "encryptedData"> & {
    __type: "handshake";
  };

export class HandshakeRepository {
  constructor(
    readonly db: KasiaDB,
    readonly tenantId: string,
    readonly walletPassword: string
  ) {}

  async getHandshake(handshakeId: string): Promise<Handshake> {
    const result = await this.db.get("handshakes", handshakeId);

    if (!result) {
      throw new DBNotFoundException();
    }

    return this._dbHandshakeToHandshake(result);
  }

  async doesExistsById(handshakeId: string): Promise<boolean> {
    return this.db.count("handshakes", handshakeId).then((count) => count > 0);
  }

  async getHandshakes(): Promise<Handshake[]> {
    return this.db
      .getAllFromIndex("handshakes", "by-tenant-id", this.tenantId)
      .then((dbHandshakes) => {
        return dbHandshakes.map((dbHandshake) => {
          return this._dbHandshakeToHandshake(dbHandshake);
        });
      });
  }

  async getHanshakesByConversationId(
    conversationId: string
  ): Promise<Handshake[]> {
    return this.db
      .getAllFromIndex("handshakes", "by-tenant-id-conversation-id", [
        this.tenantId,
        conversationId,
      ])
      .then((dbHandshakes) => {
        return dbHandshakes.map((dbHandshake) => {
          return this._dbHandshakeToHandshake(dbHandshake);
        });
      });
  }

  /**
   * returns null in case there is no handshakes
   */
  async getLastDbHandshakesByCreatedAt(): Promise<DbHandshake | null> {
    const cursor = await this.db
      .transaction("handshakes", "readonly")
      .objectStore("handshakes")
      .index("by-tenant-id-created-at")
      .openCursor(IDBKeyRange.upperBound([this.tenantId, new Date()]), "prev");

    if (!cursor) {
      return null;
    }

    return cursor.value;
  }

  async saveHandshake(handshake: Omit<Handshake, "tenantId">): Promise<void> {
    await this.db.put(
      "handshakes",
      this._handshakeToDbHandshake({
        ...handshake,
        tenantId: this.tenantId,
      })
    );
    return;
  }

  async deleteHandshake(handshakeId: string): Promise<void> {
    await this.db.delete("handshakes", handshakeId);
    return;
  }

  async saveBulk(handshakes: Omit<Handshake, "tenantId">[]): Promise<void> {
    const tx = this.db.transaction("handshakes", "readwrite");
    const store = tx.objectStore("handshakes");

    for (const handshake of handshakes) {
      // Check if handshake already exists
      const existing = await store.get(
        `${this.tenantId}_${handshake.transactionId}`
      );
      if (!existing) {
        await store.put(
          this._handshakeToDbHandshake({
            ...handshake,
            tenantId: this.tenantId,
          })
        );
      }
    }

    await tx.done;
  }

  async reEncrypt(newPassword: string): Promise<void> {
    const transaction = this.db.transaction("handshakes", "readwrite");
    const store = transaction.objectStore("handshakes");
    const index = store.index("by-tenant-id");
    const cursor = await index.openCursor(IDBKeyRange.only(this.tenantId));

    if (!cursor) {
      return;
    }

    do {
      const dbHandshake = cursor.value;
      // Decrypt with old password
      const decryptedData = decryptXChaCha20Poly1305(
        dbHandshake.encryptedData,
        this.walletPassword
      );
      // Re-encrypt with new password
      const reEncryptedData = encryptXChaCha20Poly1305(
        decryptedData,
        newPassword
      );
      // Update in database
      await cursor.update({
        ...dbHandshake,
        encryptedData: reEncryptedData,
      });
    } while (await cursor.continue());
  }

  async deleteTenant(tenantId: string): Promise<void> {
    const keys = await this.db.getAllKeysFromIndex(
      "handshakes",
      "by-tenant-id",
      tenantId
    );

    await Promise.all(keys.map((k) => this.db.delete("handshakes", k)));
  }

  private _handshakeToDbHandshake(handshake: Handshake): DbHandshake {
    return {
      id: `${handshake.tenantId}_${handshake.transactionId}`,
      transactionId: handshake.transactionId,
      conversationId: handshake.conversationId,
      createdAt: handshake.createdAt,
      contactId: handshake.contactId,
      encryptedData: encryptXChaCha20Poly1305(
        JSON.stringify({
          amount: handshake.amount,
          fee: handshake.fee,
          content: handshake.content,
          fromMe: handshake.fromMe,
        } satisfies HandshakeBag),
        this.walletPassword
      ),
      tenantId: this.tenantId,
    };
  }

  private _dbHandshakeToHandshake(dbHandshake: DbHandshake): Handshake {
    const handshakeBag = JSON.parse(
      decryptXChaCha20Poly1305(dbHandshake.encryptedData, this.walletPassword)
    ) as HandshakeBag;

    return {
      id: dbHandshake.id,
      tenantId: dbHandshake.tenantId,
      contactId: dbHandshake.contactId,
      conversationId: dbHandshake.conversationId,
      createdAt: dbHandshake.createdAt,
      transactionId: dbHandshake.transactionId,
      fee: handshakeBag.fee,
      amount: handshakeBag.amount,
      content: handshakeBag.content,
      fromMe: handshakeBag.fromMe,
      __type: "handshake",
    };
  }
}
