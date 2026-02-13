use crate::imports::*;
use kaspa_addresses::{Prefix, Version};

pub fn generate_rothschild_credentials(network: Network) -> (String, String) {
    let prefix = match network {
        Network::Mainnet => Prefix::Mainnet,
        Network::Testnet12 => Prefix::Testnet,
    };

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
    let prefix = match network {
        Network::Mainnet => Prefix::Mainnet,
        Network::Testnet12 => Prefix::Testnet,
    };

    let key_bytes =
        Vec::from_hex(private_key_hex.trim()).map_err(|err| Error::custom(err.to_string()))?;
    let secret_key = secp256k1::SecretKey::from_slice(&key_bytes)
        .map_err(|err| Error::custom(err.to_string()))?;
    let public_key = secp256k1::PublicKey::from_secret_key(&secp256k1::SECP256K1, &secret_key);
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
