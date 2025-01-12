use std::fs;
use std::hash::Hash;
use crate::data::data_store::DataStore;
use crate::{genesis, util};
use crate::schema::structs::{Block, NetworkEnvironment, Transaction};
use bitcoin::secp256k1::PublicKey;
use eframe::egui::TextBuffer;
use redgold_schema::constants::{
    DEBUG_FINALIZATION_INTERVAL_MILLIS, OBSERVATION_FORMATION_TIME_MILLIS,
    REWARD_POLL_INTERVAL, STANDARD_FINALIZATION_INTERVAL_MILLIS,
};
use redgold_schema::util::mnemonic_words::MnemonicWords;
use std::path::{Path, PathBuf};
use std::time::Duration;
use itertools::Itertools;
use log::{debug, info};
use redgold_schema::servers::Server;
use redgold_schema::{ErrorInfoContext, ShortString, structs};
use redgold_schema::structs::{Address, DynamicNodeMetadata, ErrorInfo, NodeMetadata, NodeType, PeerData, PeerId, PeerIdInfo, PeerNodeInfo, Request, Response, VersionInfo};
use redgold_schema::transaction_builder::TransactionBuilder;
use redgold_schema::util::{dhash_vec, merkle};
use redgold_schema::util::merkle::MerkleTree;
use crate::api::public_api::PublicClient;
use crate::core::seeds::SeedNode;
use crate::util::cli::args::RgArgs;
use crate::util::cli::commands;
use crate::util::cli::data_folder::{DataFolder, EnvDataFolder};

pub struct CanaryConfig {}

#[derive(Clone, Debug)]
pub struct GenesisConfig {
    block: Block,
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            block: genesis::create_genesis_block(),
        }
    }
}

// TODO: put the default node configs here
#[derive(Clone, Debug)]
pub struct NodeConfig {
    // User supplied params
    // TODO: Should this be a class Peer_ID with a multihash of the top level?
    // TODO: Review all schemas to see if we can switch to multiformats types.
    pub self_peer_id: Vec<u8>,
    // TODO: Change to Seed class? or maybe not leave it as it's own
    pub mnemonic_words: String,
    // Sometimes adjusted user params
    pub port_offset: u16,
    pub p2p_port: Option<u16>,
    pub control_port: Option<u16>,
    pub public_port: Option<u16>,
    pub rosetta_port: Option<u16>,
    pub disable_control_api: bool,
    pub disable_public_api: bool,
    // Rarely adjusted user suppliable params
    pub seed_hosts: Vec<String>,
    // Custom debug only network params
    pub observation_formation_millis: Duration,
    pub transaction_finalization_time: Duration,
    pub reward_poll_interval_secs: u64,
    pub network: NetworkEnvironment,
    pub check_observations_done_poll_interval: Duration,
    pub check_observations_done_poll_attempts: u64,
    pub seeds: Vec<SeedNode>,
    pub executable_checksum: Option<String>,
    pub disable_auto_update: bool,
    pub auto_update_poll_interval: Duration,
    pub block_formation_interval: Duration,
    pub genesis_config: GenesisConfig,
    pub faucet_enabled: bool,
    pub e2e_enabled: bool,
    pub load_balancer_url: String,
    pub external_ip: String,
    pub servers: Vec<Server>,
    pub log_level: String,
    pub data_folder: DataFolder,
    pub secure_data_folder: Option<DataFolder>,
    pub enable_logging: bool,
    pub discovery_interval: Duration,
    pub live_e2e_interval: Duration,
    pub genesis: bool,
    pub opts: RgArgs
}

impl NodeConfig {

    pub fn env_data_folder(&self) -> EnvDataFolder {
        self.data_folder.by_env(self.network)
    }

    pub fn data_store_path(&self) -> String {
        self.env_data_folder().data_store_path().to_str().unwrap().to_string()
    }

    // TODO: this can be fixed at arg parse time
    pub fn public_key(&self) -> structs::PublicKey {
        let pair = self.internal_mnemonic().active_keypair();
        let pk_vec = pair.public_key_vec();
        structs::PublicKey::from_bytes(pk_vec)
    }

    pub fn short_id(&self) -> Result<String, ErrorInfo> {
        self.public_key().hex()?.short_string()
    }

    pub fn version_info(&self) -> VersionInfo {
        VersionInfo{
            executable_checksum: self.executable_checksum.clone().unwrap_or("".to_string()),
            commit_hash: None,
            next_upgrade_time: None,
            next_executable_checksum: None,
        }
    }

    pub fn node_metadata(&self) -> NodeMetadata {
        let pair = self.internal_mnemonic().active_keypair();
        let pk_vec = pair.public_key_vec();
        NodeMetadata{
            external_address: self.external_ip.clone(),
            multi_hash: util::to_libp2p_peer_id_ser(&pk_vec).to_bytes(),
            public_key: Some(self.public_key()),
            proof: None,
            node_type: Some(NodeType::Static as i32),
            version_info: Some(self.version_info()),
            partition_info: None,
            port_offset: Some(self.port_offset as i64),
            alias: None,
            name: None,
            peer_id: Some(PeerId::from_bytes(self.self_peer_id.clone())),
            nat_restricted: None,
            // network_environment: self.network as i32,
            network_environment: self.network.clone() as i32
        }
    }

    pub fn request(&self) -> Request {
        let mut req = Request::empty();
        req.with_auth(&self.internal_mnemonic().active_keypair()).with_metadata(self.node_metadata()).clone()
    }

    pub fn response(&self) -> Response {
        let mut req = Response::empty_success();
        req.with_auth(&self.internal_mnemonic().active_keypair()).with_metadata(self.node_metadata()).clone()
    }

    pub fn dynamic_node_metadata(&self) -> Option<DynamicNodeMetadata> {
        // TODO: Load from config
        // self.data_store()
        None
    }

    pub fn self_peer_info(&self) -> PeerNodeInfo {
        PeerNodeInfo {
            latest_peer_transaction: Some(self.peer_data_tx()),
            latest_node_transaction: Some(self.peer_node_data_tx()),
            dynamic_node_metadata: self.dynamic_node_metadata(),
        }
    }

    pub fn self_peer_id_info(&self) -> PeerIdInfo {
        PeerIdInfo {
            latest_peer_transaction: Some(self.peer_data_tx()),
            peer_node_info: vec![self.self_peer_info()],
        }
    }

    pub fn peer_data_tx(&self) -> Transaction {
        let pair = self.internal_mnemonic().active_keypair();

        let pd = PeerData {
            peer_id: Some(PeerId::from_bytes(self.self_peer_id.clone())),
            merkle_proof: None,
            proof: None,
            node_metadata: vec![self.node_metadata()],
            labels: vec![],
            version_info: Some(self.version_info())
        };

        let tx = TransactionBuilder::new().with_output_peer_data(
            &pair.address_typed(), pd
        ).transaction.clone();
        tx
    }

    pub fn peer_node_data_tx(&self) -> Transaction {
        let pair = self.internal_mnemonic().active_keypair();

        let tx = TransactionBuilder::new().with_output_node_metadata(
            &pair.address_typed(), self.node_metadata()
        ).transaction.clone();
        tx
    }

    pub fn lb_client(&self) -> PublicClient {
        let vec = self.load_balancer_url.split(":").collect_vec();
        let last = vec.get(vec.len() - 1).unwrap().to_string();
        let maybe_port = last.parse::<u16>();
        let (host, port) = match maybe_port {
            Ok(p) => {
                (vec.join(":").to_string(), p)
            },
            Err(_) => {
                (self.load_balancer_url.clone(), self.network.default_port_offset() + 1)
            }
        };
        info!("Load balancer host: {} port: {:?}", host, port);
        PublicClient::from(host, port)
    }

    pub fn is_local_debug(&self) -> bool {
        self.network == NetworkEnvironment::Local || self.network == NetworkEnvironment::Debug
    }

    pub fn is_debug(&self) -> bool {
        self.network == NetworkEnvironment::Debug
    }

    pub fn main_stage_network(&self) -> bool {
        self.network == NetworkEnvironment::Main ||
        self.network == NetworkEnvironment::Test ||
        self.network == NetworkEnvironment::Staging ||
        self.network == NetworkEnvironment::Dev ||
            self.network == NetworkEnvironment::Predev
    }

    pub fn address(&self) -> Address {
        Address::from_public(&self.internal_mnemonic().active_keypair().public_key).expect("address")
    }

    pub fn genesis_transaction(&self) -> Transaction {
        self.genesis_config
            .block
            .transactions
            .get(0)
            .expect("filled")
            .clone()
    }

    pub fn p2p_port(&self) -> u16 {
        self.p2p_port.unwrap_or(self.port_offset + 0)
    }

    pub fn public_port(&self) -> u16 {
        self.public_port.unwrap_or(self.port_offset + 1)
    }

    pub fn control_port(&self) -> u16 {
        self.control_port.unwrap_or(self.port_offset + 2)
    }

    pub fn rosetta_port(&self) -> u16 {
        self.rosetta_port.unwrap_or(self.port_offset + 3)
    }

    pub fn mparty_port(&self) -> u16 {
        self.port_offset + 4
    }

    pub fn udp_port(&self) -> u16 {
        self.port_offset + 5
    }

    pub fn explorer_port(&self) -> u16 {
        self.port_offset + 6
    }

    pub fn default_debug() -> Self {
        NodeConfig::from_test_id(&(0 as u16))
    }

    pub fn default() -> Self {
        Self {
            self_peer_id: vec![],
            mnemonic_words: "".to_string(),
            port_offset: NetworkEnvironment::Debug.default_port_offset(),
            p2p_port: None,
            control_port: None,
            public_port: None,
            rosetta_port: None,
            disable_control_api: false,
            disable_public_api: false,
            seed_hosts: vec![],
            observation_formation_millis: Duration::from_millis(OBSERVATION_FORMATION_TIME_MILLIS),
            transaction_finalization_time: Duration::from_millis(
                STANDARD_FINALIZATION_INTERVAL_MILLIS,
            ),
            reward_poll_interval_secs: REWARD_POLL_INTERVAL,
            network: NetworkEnvironment::Debug,
            check_observations_done_poll_interval: Duration::from_secs(1),
            check_observations_done_poll_attempts: 3,
            seeds: vec![],
            executable_checksum: None,
            disable_auto_update: false,
            auto_update_poll_interval: Duration::from_secs(60),
            block_formation_interval: Duration::from_secs(10),
            genesis_config: Default::default(),
            faucet_enabled: true,
            e2e_enabled: true,
            load_balancer_url: "lb.redgold.io".to_string(),
            external_ip: "127.0.0.1".to_string(),
            servers: vec![],
            log_level: "DEBUG".to_string(),
            data_folder: DataFolder::target(0),
            secure_data_folder: None,
            enable_logging: true,
            discovery_interval: Duration::from_secs(5),
            live_e2e_interval: Duration::from_secs(60),
            genesis: false,
            opts: RgArgs::default(),
        }
    }

    pub fn memdb_path(seed_id: &u16) -> String {
        "file:memdb1_id".to_owned() + &*seed_id.clone().to_string() + "?mode=memory&cache=shared"
    }

    pub fn from_test_id(seed_id: &u16) -> Self {
        let words = redgold_schema::util::mnemonic_builder::from_str_rounds(
            &*seed_id.clone().to_string(),
            0,
        )
        .to_string();
        let self_peer_id = peer_id_from_single_mnemonic(words.clone())
            .expect("")
            .root
            .vec();
        // let path: String = ""
        let folder = DataFolder::target(seed_id.clone() as u32);
        folder.delete().ensure_exists();
        // folder.ensure_exists();
        let mut node_config = NodeConfig::default();
        node_config.self_peer_id = self_peer_id;
        node_config.mnemonic_words = words;
        node_config.port_offset = (node_config.port_offset + (seed_id.clone() * 100)) as u16;
        node_config.data_folder = folder;
        node_config.observation_formation_millis = Duration::from_millis(1000 as u64);
        node_config.transaction_finalization_time =
            Duration::from_millis(DEBUG_FINALIZATION_INTERVAL_MILLIS);
        node_config.network = NetworkEnvironment::Debug;
        node_config.check_observations_done_poll_interval = Duration::from_secs(1);
        node_config.check_observations_done_poll_attempts = 5;
        node_config.e2e_enabled = false;
        node_config
    }
    pub fn internal_mnemonic(&self) -> MnemonicWords {
        MnemonicWords::from_mnemonic_words(&*self.mnemonic_words, None)
    }

    pub async fn data_store(&self) -> DataStore {
        DataStore::from_config(self).await
    }

    pub async fn data_store_all(&self) -> DataStore {
        let all = self.data_folder.all().data_store_path();
        DataStore::from_file_path(all.to_str().expect("failed to render ds path").to_string()).await
    }

    pub async fn data_store_all_from(top_level_folder: String) -> DataStore {
        let p = PathBuf::from(top_level_folder.clone());
        let all = p.join(NetworkEnvironment::All.to_std_string());
        DataStore::from_file_path(all.to_str().expect("failed to render ds path").to_string()).await
    }

    pub async fn data_store_all_secure(&self) -> Option<DataStore> {
        // TODO: Move to arg translate
        if let Some(sd) = std::env::var(commands::REDGOLD_SECURE_DATA_PATH).ok() {
            Some(Self::data_store_all_from(sd).await)
        } else {
            None
        }
    }

    pub fn secure_path(&self) -> Option<String> {
        // TODO: Move to arg translate
        std::env::var(commands::REDGOLD_SECURE_DATA_PATH).ok()
    }

    pub fn secure_all_path(&self) -> Option<String> {
        // TODO: Move to arg translate
        std::env::var(commands::REDGOLD_SECURE_DATA_PATH).ok().map(|p| {
            let buf = PathBuf::from(p);
            buf.join(NetworkEnvironment::All.to_std_string())
        }).map(|p| p.to_str().expect("failed to render ds path").to_string())
    }

    pub fn secure_mnemonic(&self) -> Option<String> {
        self.secure_all_path().and_then(|p| {
            fs::read_to_string(p).ok()
        })
    }

    pub async fn loopback_public_client(&self) -> PublicClient {
        PublicClient::from("127.0.0.1".to_string(), self.public_port())
    }

}

// TODO: Update function!
pub fn peer_id_from_single_mnemonic(mnemonic_words: String) -> Result<MerkleTree, ErrorInfo> {
    let wallet = MnemonicWords::from_mnemonic_words(&*mnemonic_words, None);
    let (_, pk) = wallet.active_key();
    let h = structs::Hash::digest(pk.serialize().to_vec());
    merkle::build_root(vec![h])
}

#[test]
fn debug(){

}