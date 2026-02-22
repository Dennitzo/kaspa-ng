use crate::models::types::hash::Hash;

#[derive(Clone)]
pub struct ScriptTransaction {
    pub script_public_key: Vec<u8>,
    pub transaction_id: Hash,
    pub block_time: i64,
}

impl Eq for ScriptTransaction {}

impl PartialEq for ScriptTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.script_public_key == other.script_public_key && self.transaction_id == other.transaction_id
    }
}

impl std::hash::Hash for ScriptTransaction {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.script_public_key.hash(state);
        self.transaction_id.hash(state);
    }
}
