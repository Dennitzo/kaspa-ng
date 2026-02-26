import { encryptXChaCha20Poly1305, decryptXChaCha20Poly1305 } from "kaspa-wasm";
import { KasiaDB, DBNotFoundException } from "./db";

export type DbBroadcastChannel = {
  /**
   * unique uuidv4
   */
  id: string;
  /**
   * tenant is the selected wallet
   */
  tenantId: string;
  timestamp: Date;
  /**
   * encrypted data shaped as `json(BroadcastChannelBag)`
   */
  encryptedData: string;
};

export type BroadcastChannelBag = {
  /**
   * the channel name/key
   */
  channelName: string;
  /**
   * the channel value/content
   */
  channelValue: string;
};

export type BroadcastChannel = BroadcastChannelBag &
  Omit<DbBroadcastChannel, "encryptedData">;

export class BroadcastChannelRepository {
  constructor(
    readonly db: KasiaDB,
    readonly tenantId: string,
    readonly walletPassword: string
  ) {}

  async getBroadcastChannel(channelId: string): Promise<BroadcastChannel> {
    const result = await this.db.get("broadcastChannels", channelId);

    if (!result) {
      throw new DBNotFoundException();
    }

    return this._dbBroadcastChannelToBroadcastChannel(result);
  }

  async getBroadcastChannelByName(
    channelName: string
  ): Promise<BroadcastChannel> {
    const result = await this.getBroadcastChannels().then((channels) => {
      return channels.find((channel) => channel.channelName === channelName);
    });

    if (!result) {
      throw new DBNotFoundException();
    }

    return result;
  }

  async getBroadcastChannels(): Promise<BroadcastChannel[]> {
    return this.db
      .getAllFromIndex("broadcastChannels", "by-tenant-id", this.tenantId)
      .then((dbChannels) => {
        return dbChannels.map((dbChannel) => {
          return this._dbBroadcastChannelToBroadcastChannel(dbChannel);
        });
      });
  }

  async saveBroadcastChannel(
    channel: Omit<BroadcastChannel, "tenantId">
  ): Promise<string> {
    return this.db.put(
      "broadcastChannels",
      this._broadcastChannelToDbBroadcastChannel({
        ...channel,
        tenantId: this.tenantId,
      })
    );
  }

  async deleteBroadcastChannel(channelId: string): Promise<void> {
    await this.db.delete("broadcastChannels", channelId);
    return;
  }

  async deleteBroadcastChannelByName(channelName: string): Promise<void> {
    const channel = await this.getBroadcastChannelByName(channelName);
    await this.deleteBroadcastChannel(channel.id);
  }

  async reEncrypt(newPassword: string): Promise<void> {
    const transaction = this.db.transaction("broadcastChannels", "readwrite");
    const store = transaction.objectStore("broadcastChannels");
    const index = store.index("by-tenant-id");
    const cursor = await index.openCursor(IDBKeyRange.only(this.tenantId));
    if (!cursor) {
      return;
    }

    do {
      const dbChannel = cursor.value;
      // Decrypt with old password
      const decryptedData = decryptXChaCha20Poly1305(
        dbChannel.encryptedData,
        this.walletPassword
      );
      // Re-encrypt with new password
      const reEncryptedData = encryptXChaCha20Poly1305(
        decryptedData,
        newPassword
      );
      // Update in database
      await cursor.update({
        ...dbChannel,
        encryptedData: reEncryptedData,
      });
    } while (await cursor.continue());
  }

  async deleteTenant(tenantId: string): Promise<void> {
    const keys = await this.db.getAllKeysFromIndex(
      "broadcastChannels",
      "by-tenant-id",
      tenantId
    );

    await Promise.all(keys.map((k) => this.db.delete("broadcastChannels", k)));
  }

  private _broadcastChannelToDbBroadcastChannel(
    channel: BroadcastChannel
  ): DbBroadcastChannel {
    return {
      id: channel.id,
      encryptedData: encryptXChaCha20Poly1305(
        JSON.stringify({
          channelName: channel.channelName,
          channelValue: channel.channelValue,
        } satisfies BroadcastChannelBag),
        this.walletPassword
      ),
      timestamp: channel.timestamp,
      tenantId: this.tenantId,
    };
  }

  private _dbBroadcastChannelToBroadcastChannel(
    dbChannel: DbBroadcastChannel
  ): BroadcastChannel {
    const channelBag = JSON.parse(
      decryptXChaCha20Poly1305(dbChannel.encryptedData, this.walletPassword)
    ) as BroadcastChannelBag;

    return {
      tenantId: dbChannel.tenantId,
      id: dbChannel.id,
      timestamp: dbChannel.timestamp,
      channelName: channelBag.channelName,
      channelValue: channelBag.channelValue,
    };
  }
}
