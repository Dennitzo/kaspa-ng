import { encryptXChaCha20Poly1305, decryptXChaCha20Poly1305 } from "kaspa-wasm";
import { KasiaDB, DBNotFoundException } from "./db";

export type DbContact = {
  /**
   * `uuidv4()`
   */
  id: string;
  /**
   * tenant is the selected wallet
   */
  tenantId: string;
  timestamp: Date;
  /**
   * encrypted data shaped as `json(ContactBag)`
   */
  encryptedData: string;
};

export type ContactBag = {
  name?: string;
  kaspaAddress: string;
};
export type Contact = ContactBag & Omit<DbContact, "encryptedData">;

export class ContactRepository {
  constructor(
    readonly db: KasiaDB,
    readonly tenantId: string,
    readonly walletPassword: string
  ) {}

  async getContact(contactId: string): Promise<Contact> {
    const result = await this.db.get("contacts", contactId);

    if (!result) {
      throw new DBNotFoundException();
    }

    return this._dbContactToContact(result);
  }

  async getContactByKaspaAddress(kaspaAddress: string): Promise<Contact> {
    const result = await this.getContacts().then((contacts) => {
      return contacts.find((contact) => contact.kaspaAddress === kaspaAddress);
    });

    if (!result) {
      throw new DBNotFoundException();
    }

    return result;
  }

  async getContacts(): Promise<Contact[]> {
    return this.db
      .getAllFromIndex("contacts", "by-tenant-id", this.tenantId)
      .then((dbContacts) => {
        return dbContacts.map((dbContact) => {
          return this._dbContactToContact(dbContact);
        });
      });
  }

  async saveContact(contact: Omit<Contact, "tenantId">): Promise<string> {
    console.log(
      this._contactToDbContact({ ...contact, tenantId: this.tenantId })
    );
    return this.db.put(
      "contacts",
      this._contactToDbContact({ ...contact, tenantId: this.tenantId })
    );
  }

  async deleteContact(contactId: string): Promise<void> {
    await this.db.delete("contacts", contactId);
    return;
  }

  async saveBulk(contacts: Omit<Contact, "tenantId">[]): Promise<void> {
    const tx = this.db.transaction("contacts", "readwrite");
    const store = tx.objectStore("contacts");

    for (const contact of contacts) {
      // Check if contact already exists
      const existing = await store.get(contact.id);
      if (!existing) {
        await store.put(
          this._contactToDbContact({ ...contact, tenantId: this.tenantId })
        );
      }
    }

    await tx.done;
  }

  async reEncrypt(newPassword: string): Promise<void> {
    const transaction = this.db.transaction("contacts", "readwrite");
    const store = transaction.objectStore("contacts");
    const index = store.index("by-tenant-id");
    const cursor = await index.openCursor(IDBKeyRange.only(this.tenantId));
    if (!cursor) {
      return;
    }

    do {
      const dbContact = cursor.value;
      // Decrypt with old password
      const decryptedData = decryptXChaCha20Poly1305(
        dbContact.encryptedData,
        this.walletPassword
      );
      // Re-encrypt with new password
      const reEncryptedData = encryptXChaCha20Poly1305(
        decryptedData,
        newPassword
      );
      // Update in database
      await cursor.update({
        ...dbContact,
        encryptedData: reEncryptedData,
      });
    } while (await cursor.continue());
  }

  async deleteTenant(tenantId: string): Promise<void> {
    const keys = await this.db.getAllKeysFromIndex(
      "contacts",
      "by-tenant-id",
      tenantId
    );

    await Promise.all(keys.map((k) => this.db.delete("contacts", k)));
  }

  private _contactToDbContact(contact: Contact): DbContact {
    return {
      id: contact.id,
      encryptedData: encryptXChaCha20Poly1305(
        JSON.stringify({
          name: contact.name,
          kaspaAddress: contact.kaspaAddress,
        } satisfies ContactBag),
        this.walletPassword
      ),
      timestamp: contact.timestamp,
      tenantId: this.tenantId,
    };
  }

  private _dbContactToContact(dbContact: DbContact): Contact {
    const contactBag = JSON.parse(
      decryptXChaCha20Poly1305(dbContact.encryptedData, this.walletPassword)
    ) as ContactBag;

    return {
      tenantId: dbContact.tenantId,
      id: dbContact.id,
      timestamp: dbContact.timestamp,
      name: contactBag.name,
      kaspaAddress: contactBag.kaspaAddress,
    };
  }
}
