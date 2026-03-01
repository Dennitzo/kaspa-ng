use crate::imports::*;
use kaspa_addresses::Prefix as AddressPrefix;
use kaspa_consensus_core::config::params::Params;
use kaspa_wallet_core::utxo::NetworkParams;

pub const BASIC_TRANSACTION_MASS: u64 = 2036;

#[derive(Default, Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Network {
    #[default]
    Mainnet,
}

impl<'de> Deserialize<'de> for Network {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        match value.trim().to_ascii_lowercase().as_str() {
            "mainnet" => Ok(Network::Mainnet),
            _ => Err(serde::de::Error::custom(format!(
                "invalid network value: {}",
                value
            ))),
        }
    }
}

impl std::fmt::Display for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _ = self;
        write!(f, "mainnet")
    }
}

impl FromStr for Network {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mainnet" => Ok(Network::Mainnet),
            _ => Err(Error::InvalidNetwork(s.to_string())),
        }
    }
}

impl From<Network> for NetworkType {
    fn from(network: Network) -> Self {
        let _ = network;
        NetworkType::Mainnet
    }
}

impl From<&Network> for NetworkType {
    fn from(network: &Network) -> Self {
        let _ = network;
        NetworkType::Mainnet
    }
}

impl From<Network> for NetworkId {
    fn from(network: Network) -> Self {
        let _ = network;
        NetworkId::new(NetworkType::Mainnet)
    }
}

impl From<&Network> for AddressPrefix {
    fn from(network: &Network) -> Self {
        NetworkType::from(network).into()
    }
}

impl From<Network> for AddressPrefix {
    fn from(network: Network) -> Self {
        NetworkType::from(network).into()
    }
}

impl From<&Network> for NetworkId {
    fn from(network: &Network) -> Self {
        let _ = network;
        NetworkId::new(NetworkType::Mainnet)
    }
}

impl From<NetworkId> for Network {
    fn from(value: NetworkId) -> Self {
        match value.network_type {
            NetworkType::Mainnet => Network::Mainnet,
            NetworkType::Testnet => Network::Mainnet,
            NetworkType::Devnet => unreachable!("Devnet is not supported"),
            NetworkType::Simnet => unreachable!("Simnet is not supported"),
        }
    }
}

impl From<Network> for Params {
    fn from(network: Network) -> Self {
        NetworkId::from(network).into()
    }
}

impl From<&Network> for Params {
    fn from(network: &Network) -> Self {
        NetworkId::from(network).into()
    }
}

impl From<Network> for &'static NetworkParams {
    fn from(network: Network) -> Self {
        NetworkParams::from(NetworkId::from(network))
    }
}

impl From<&Network> for &'static NetworkParams {
    fn from(network: &Network) -> Self {
        NetworkParams::from(NetworkId::from(network))
    }
}

const NETWORKS: [Network; 1] = [Network::Mainnet];

impl Network {
    pub fn iter() -> impl Iterator<Item = &'static Network> {
        NETWORKS.iter()
    }

    pub fn name(&self) -> &str {
        let _ = self;
        i18n("Mainnet")
    }

    pub fn describe(&self) -> &str {
        let _ = self;
        i18n("Main Kaspa network")
    }

    pub fn tps(&self) -> u64 {
        let params = Params::from(*self);
        params.max_block_mass / BASIC_TRANSACTION_MASS * params.bps_history().after()
    }
}

const MAX_NETWORK_PRESSURE_SAMPLES: usize = 16;
const NETWORK_PRESSURE_ALPHA_HIGH: f32 = 0.8;
const NETWORK_PRESSURE_ALPHA_LOW: f32 = 0.5;
const NETWORK_PRESSURE_THRESHOLD_HIGH: f32 = 0.4;
const NETWORK_PRESSURE_THRESHOLD_LOW: f32 = 0.2;
const NETWORK_CAPACITY_THRESHOLD: usize = 90;

#[derive(Default, Debug, Clone)]
pub struct NetworkPressure {
    pub network_pressure_samples: VecDeque<f32>,
    pub pressure: f32,
    pub is_high: bool,
}

impl NetworkPressure {
    pub fn clear(&mut self) {
        self.network_pressure_samples.clear();
        self.pressure = 0.0;
    }

    fn insert_sample(&mut self, pressure: f32, alpha: f32) {
        let pressure = alpha * pressure + (1.0 - alpha) * self.pressure;
        self.network_pressure_samples.push_back(pressure);
        if self.network_pressure_samples.len() > MAX_NETWORK_PRESSURE_SAMPLES {
            self.network_pressure_samples.pop_front();
        }
    }

    pub fn update_mempool_size(&mut self, mempool_size: usize, network: &Network) {
        let pressure = mempool_size as f32 / network.tps() as f32;

        if pressure > self.pressure {
            self.insert_sample(pressure, NETWORK_PRESSURE_ALPHA_HIGH);
        } else {
            self.insert_sample(pressure, NETWORK_PRESSURE_ALPHA_LOW);
        }

        let average_pressure = self.network_pressure_samples.iter().sum::<f32>()
            / self.network_pressure_samples.len() as f32;

        self.pressure = average_pressure;
        if self.is_high {
            self.is_high = self.pressure > NETWORK_PRESSURE_THRESHOLD_LOW;
        } else {
            self.is_high = self.pressure > NETWORK_PRESSURE_THRESHOLD_HIGH;
        }
    }

    pub fn is_high(&self) -> bool {
        self.is_high
    }

    pub fn pressure(&self) -> f32 {
        self.pressure
    }

    pub fn capacity(&self) -> usize {
        (self.pressure * 100.0).min(100.0) as usize
    }

    pub fn above_capacity(&self) -> bool {
        self.capacity() > NETWORK_CAPACITY_THRESHOLD
    }

    pub fn below_capacity(&self) -> bool {
        self.capacity() <= NETWORK_CAPACITY_THRESHOLD
    }
}
