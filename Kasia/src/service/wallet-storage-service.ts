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

  constructor() {
    // Initialize wallets array if it doesn't exist
    if (!localStorage.getItem(this._storageKey)) {
      localStorage.setItem(this._storageKey, JSON.stringify([]));
    }
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
    const walletsString = localStorage.getItem(this._storageKey);
    if (!walletsString) return [];
    const wallets = JSON.parse(walletsString) as StoredWallet[];
    return wallets.map(({ id, name, createdAt }) => ({
      id,
      name,
      createdAt,
    }));
  }

  getDecrypted(walletId: string, password: string): UnlockedWallet {
    const walletsString = localStorage.getItem(this._storageKey);
    if (!walletsString) throw new Error("No wallets found");

    const wallets = JSON.parse(walletsString) as StoredWallet[];
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
    const walletsString = localStorage.getItem(this._storageKey);
    if (!walletsString) throw new Error("Storage not initialized");

    const wallets = JSON.parse(walletsString) as StoredWallet[];

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
    localStorage.setItem(this._storageKey, JSON.stringify(wallets));
    return newWallet.id;
  }

  deleteWallet(walletId: string) {
    const walletsString = localStorage.getItem(this._storageKey);
    if (!walletsString) return;

    const wallets = JSON.parse(walletsString) as StoredWallet[];
    const updatedWallets = wallets.filter((w) => w.id !== walletId);
    localStorage.setItem(this._storageKey, JSON.stringify(updatedWallets));

    // clean-up messaging-related localStorage keys
    localStorage.removeItem(`kasia_last_opened_contact_${walletId}`);
    localStorage.removeItem(`kasia_last_opened_channel_${walletId}`);
    localStorage.removeItem(`metadata_${walletId}`);
  }

  isInitialized() {
    const walletsString = localStorage.getItem(this._storageKey);
    if (!walletsString) return false;
    const wallets = JSON.parse(walletsString) as StoredWallet[];
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
    const walletsString = localStorage.getItem(this._storageKey);
    if (!walletsString) throw new Error("No wallets found");

    const wallets = JSON.parse(walletsString) as StoredWallet[];
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
      localStorage.setItem(this._storageKey, JSON.stringify(updatedWallets));
    } catch (error) {
      console.error("Error changing password:", error);
      throw new Error("Invalid current password");
    }
  }

  /**
   * Change the name of an existing wallet
   */
  changeWalletName(walletId: string, newName: string): void {
    const walletsString = localStorage.getItem(this._storageKey);
    if (!walletsString) throw new Error("No wallets found");

    const wallets = JSON.parse(walletsString) as StoredWallet[];
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
    localStorage.setItem(this._storageKey, JSON.stringify(updatedWallets));
  }
}
