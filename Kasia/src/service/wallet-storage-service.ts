import {
  decryptXChaCha20Poly1305,
  encryptXChaCha20Poly1305,
  Mnemonic,
  PrivateKey,
  PrivateKeyGenerator,
  XPrv,
} from "kaspa-wasm";
import { v4 as uuidv4 } from "uuid";
import { StoredWallet, UnlockedWallet } from "src/types/wallet.type";

export class WalletStorageService {
  private _storageKey: string = "wallets";
  private _backupStorageKey: string = "wallets_backup";

  constructor() {
    this.ensureStorageInitialized();
  }

  private parseWallets(raw: string | null): StoredWallet[] | null {
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed) ? (parsed as StoredWallet[]) : null;
    } catch (error) {
      console.warn("Failed to parse wallet storage payload:", error);
      return null;
    }
  }

  private readWallets(): StoredWallet[] {
    const primary = this.parseWallets(localStorage.getItem(this._storageKey));
    if (primary) return primary;

    const backup = this.parseWallets(localStorage.getItem(this._backupStorageKey));
    if (backup) {
      this.writeWallets(backup);
      return backup;
    }

    this.writeWallets([]);
    return [];
  }

  private writeWallets(wallets: StoredWallet[]): void {
    const serialized = JSON.stringify(wallets);
    localStorage.setItem(this._storageKey, serialized);
    localStorage.setItem(this._backupStorageKey, serialized);
  }

  private ensureStorageInitialized(): void {
    const primary = this.parseWallets(localStorage.getItem(this._storageKey));
    const backup = this.parseWallets(localStorage.getItem(this._backupStorageKey));

    if (primary) {
      localStorage.setItem(this._backupStorageKey, JSON.stringify(primary));
      return;
    }
    if (backup) {
      localStorage.setItem(this._storageKey, JSON.stringify(backup));
      return;
    }

    this.writeWallets([]);
  }

  static getPrivateKey(
    wallet: Pick<UnlockedWallet, "encryptedPrivateKey" | "password">
  ): PrivateKey {
    try {
      // decrypt the private key hex string
      const privateKeyHex = decryptXChaCha20Poly1305(
        wallet.encryptedPrivateKey,
        wallet.password
      );
      return new PrivateKey(privateKeyHex);
    } catch (error) {
      console.error("Error getting private key:", error);
      throw new Error("Invalid password");
    }
  }

  getWalletList(): {
    id: string;
    name: string;
    createdAt: string;
  }[] {
    const wallets = this.readWallets();
    return wallets.map(({ id, name, createdAt }) => ({
      id,
      name,
      createdAt,
    }));
  }

  getDecrypted(walletId: string, password: string): UnlockedWallet {
    const wallets = this.readWallets();
    const wallet = wallets.find((w) => w.id === walletId);

    if (!wallet) {
      throw new Error("Wallet not found");
    }

    try {
      // First decrypt the mnemonic phrase
      const mnemonicPhrase = decryptXChaCha20Poly1305(
        wallet.encryptedPhrase,
        password
      );
      const mnemonic = new Mnemonic(mnemonicPhrase);

      // Decrypt passphrase if it exists
      let passphrase = "";
      if (wallet.encryptedPassphrase) {
        passphrase = decryptXChaCha20Poly1305(
          wallet.encryptedPassphrase,
          password
        );
      }

      // Generate the seed with passphrase and create master extended private key
      const seed = mnemonic.toSeed(passphrase);
      const masterXPrv = new XPrv(seed);

      // get the receive private key for address 0
      const receivePrivateKey = new PrivateKeyGenerator(
        masterXPrv,
        false,
        BigInt(0)
      ).receiveKey(0);

      const receivePublicKey = receivePrivateKey.toPublicKey();

      // encrypt the private key hex string
      const encryptedPrivateKey = encryptXChaCha20Poly1305(
        receivePrivateKey.toString(),
        password
      );

      return {
        id: wallet.id,
        name: wallet.name,
        activeAccount: 1,
        encryptedPrivateKey,
        password,
        receivePublicKey,
        passphrase: passphrase || undefined,
      };
    } catch (error) {
      console.error("Error decrypting wallet:", error);
      throw new Error("Invalid password");
    }
  }

  create(
    name: string,
    mnemonic: Mnemonic,
    password: string,
    passphrase?: string
  ): string {
    const wallets = this.readWallets();

    const newWallet: StoredWallet = {
      id: uuidv4(),
      name,
      encryptedPhrase: encryptXChaCha20Poly1305(mnemonic.phrase, password),
      createdAt: new Date().toISOString(),
      accounts: [{ name: "Account 1" }],
      encryptedPassphrase: passphrase
        ? encryptXChaCha20Poly1305(passphrase, password)
        : undefined,
    };

    wallets.push(newWallet);
    this.writeWallets(wallets);
    return newWallet.id;
  }

  deleteWallet(walletId: string) {
    const wallets = this.readWallets();
    const updatedWallets = wallets.filter((w) => w.id !== walletId);
    this.writeWallets(updatedWallets);

    // clean-up messaging-related localStorage keys
    localStorage.removeItem(`kasia_last_opened_contact_${walletId}`);
    localStorage.removeItem(`kasia_last_opened_channel_${walletId}`);
    localStorage.removeItem(`metadata_${walletId}`);
  }

  isInitialized() {
    const wallets = this.readWallets();
    return wallets.length > 0;
  }

  /**
   * Change the password for an existing wallet
   */
  async changePassword(
    walletId: string,
    currentPassword: string,
    newPassword: string
  ): Promise<void> {
    const wallets = this.readWallets();
    const walletIndex = wallets.findIndex((w) => w.id === walletId);

    if (walletIndex === -1) {
      throw new Error("Wallet not found");
    }

    const wallet = wallets[walletIndex];

    try {
      // First verify the current password by decrypting the mnemonic
      const mnemonicPhrase = decryptXChaCha20Poly1305(
        wallet.encryptedPhrase,
        currentPassword
      );

      // Re-encrypt with the new password
      const newEncryptedPhrase = encryptXChaCha20Poly1305(
        mnemonicPhrase,
        newPassword
      );

      // Handle passphrase re-encryption if it exists
      let newEncryptedPassphrase = wallet.encryptedPassphrase;
      if (wallet.encryptedPassphrase) {
        const passphrase = decryptXChaCha20Poly1305(
          wallet.encryptedPassphrase,
          currentPassword
        );
        newEncryptedPassphrase = encryptXChaCha20Poly1305(
          passphrase,
          newPassword
        );
      }

      // Create a copy of wallets and update the encrypted phrase and passphrase
      const updatedWallets = [...wallets];
      updatedWallets[walletIndex] = {
        ...wallet,
        encryptedPhrase: newEncryptedPhrase,
        encryptedPassphrase: newEncryptedPassphrase,
      };

      // Save to localStorage first - if this fails, original state is preserved
      this.writeWallets(updatedWallets);
    } catch (error) {
      console.error("Error changing password:", error);
      throw new Error("Invalid current password");
    }
  }

  /**
   * Change the name of an existing wallet
   */
  changeWalletName(walletId: string, newName: string): void {
    const wallets = this.readWallets();
    const walletIndex = wallets.findIndex((w) => w.id === walletId);

    if (walletIndex === -1) {
      throw new Error("Wallet not found");
    }

    // Check if name already exists (excluding current wallet)
    const nameExists = wallets.some(
      (w, index) =>
        index !== walletIndex && w.name.toLowerCase() === newName.toLowerCase()
    );

    if (nameExists) {
      throw new Error("A wallet with this name already exists");
    }

    // Create a copy of wallets and update the name
    const updatedWallets = [...wallets];
    updatedWallets[walletIndex] = {
      ...wallets[walletIndex],
      name: newName.trim(),
    };

    // Save to localStorage first - if this fails, original state is preserved
    this.writeWallets(updatedWallets);
  }
}
