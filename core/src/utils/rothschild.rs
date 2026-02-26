use crate::imports::*;
use kaspa_addresses::{Prefix, Version};
use secp256k1::{PublicKey, SECP256K1, SecretKey};

fn network_prefix(network: Network) -> Prefix {
    match network {
        Network::Mainnet => Prefix::Mainnet,
        Network::Testnet10 | Network::Testnet12 => Prefix::Testnet,
    }
}

pub fn generate_rothschild_credentials(network: Network) -> (String, String) {
    let prefix = network_prefix(network);

    let (secret_key, public_key) = secp256k1::generate_keypair(&mut rand::thread_rng());
    let address = Address::new(
        prefix,
        Version::PubKey,
        &public_key.x_only_public_key().0.serialize(),
    );
    let private_key = secret_key.secret_bytes().to_vec().to_hex();

    (private_key, address.to_string())
}

pub fn rothschild_address_from_private_key(
    network: Network,
    private_key_hex: &str,
) -> Result<String> {
    let prefix = network_prefix(network);

    let key_bytes =
        Vec::from_hex(private_key_hex.trim()).map_err(|err| Error::custom(err.to_string()))?;
    let secret_key =
        SecretKey::from_slice(&key_bytes).map_err(|err| Error::custom(err.to_string()))?;
    let public_key = PublicKey::from_secret_key(SECP256K1, &secret_key);
    let (xonly_public_key, _) = public_key.x_only_public_key();
    let address = Address::new(prefix, Version::PubKey, &xonly_public_key.serialize());

    Ok(address.to_string())
}

pub fn rothschild_mnemonic_from_private_key(private_key_hex: &str) -> Result<String> {
    let key_bytes =
        Vec::from_hex(private_key_hex.trim()).map_err(|err| Error::custom(err.to_string()))?;
    if key_bytes.len() != 32 {
        return Err(Error::custom(
            "Private key must be 32 bytes to derive a mnemonic",
        ));
    }

    let mnemonic = Mnemonic::from_entropy(key_bytes, Language::English)?;
    Ok(mnemonic.phrase_string())
}

pub fn rothschild_private_key_from_mnemonic(mnemonic_phrase: &str) -> Result<String> {
    let phrase = mnemonic_phrase
        .split_ascii_whitespace()
        .filter(|s| s.is_not_empty())
        .collect::<Vec<&str>>()
        .join(" ");

    let mnemonic = Mnemonic::new(phrase, Language::English)?;
    let entropy = mnemonic.entropy().clone();
    if entropy.len() != 32 {
        return Err(Error::custom(
            "Rothschild mnemonic must be 24 words (32-byte entropy)",
        ));
    }
    SecretKey::from_slice(&entropy)
        .map_err(|_| Error::custom("Mnemonic entropy is not a valid secp256k1 private key"))?;

    Ok(entropy.to_hex())
}
