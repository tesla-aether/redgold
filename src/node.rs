use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use futures::stream::FuturesUnordered;
use itertools::Itertools;

use log::info;
use metrics::counter;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use redgold_schema::constants::REWARD_AMOUNT;
use redgold_schema::{bytes_data, EasyJson, error_info, ProtoSerde, SafeBytesAccess, SafeOption, structs};
use redgold_schema::structs::{ControlMultipartyKeygenResponse, ControlMultipartySigningRequest, CurrencyAmount, GetPeersInfoRequest, Hash, InitiateMultipartySigningRequest, NetworkEnvironment, PeerId, Request, Seed, State, TestContractInternalState, Transaction, TrustData, ValidationType};

use crate::api::control_api::ControlClient;
// use crate::api::p2p_io::rgnetwork::Event;
// use crate::api::p2p_io::P2P;
use crate::api::{RgHttpClient, public_api, explorer};
use crate::api::public_api::PublicClient;
use crate::api::{control_api, rosetta};
use crate::e2e::tx_submit::TransactionSubmitter;
use crate::core::{block_formation, stream_handlers};
use crate::core::block_formation::BlockFormationProcess;
use crate::core::observation::ObservationBuffer;
use crate::core::peer_event_handler::PeerOutgoingEventHandler;
use crate::core::peer_rx_event_handler::{PeerRxEventHandler, rest_peer};
use crate::core::process_transaction::TransactionProcessContext;
use crate::core::relay::Relay;
use redgold_data::data_store::DataStore;
use crate::data::download;
use crate::genesis::{create_test_genesis_transaction, genesis_transaction, genesis_tx_from, GenesisDistribution};
use crate::node_config::NodeConfig;
use crate::schema::structs::{ ControlRequest, ErrorInfo, NodeState};
use crate::schema::{ProtoHashable, WithMetadataHashable};
// use crate::trust::rewards::Rewards;
use crate::{api, e2e, util};
// use crate::mparty::mp_server::{Db, MultipartyHandler};
use crate::e2e::tx_gen::SpendableUTXO;
use crate::core::process_observation::ObservationHandler;
use crate::multiparty::gg20_sm_manager;
use crate::util::runtimes::build_runtime;
use crate::util::{auto_update, keys, metrics_registry};
use crate::schema::constants::EARLIEST_TIME;
use redgold_keys::TestConstants;
use crate::util::trace_setup::init_tracing;
use tokio::task::spawn_blocking;
use tracing::Span;
use redgold_keys::proof_support::ProofSupport;
use redgold_schema::structs::TransactionState::Mempool;
use crate::core::contract::contract_state_manager::ContractStateManager;
use crate::core::data_discovery::DataDiscovery;
use crate::core::discovery::{Discovery, DiscoveryMessage};
use crate::core::internal_message::SendErrorInfo;
use crate::core::recent_download::RecentDownload;
use crate::core::stream_handlers::IntervalFold;
use crate::core::transact::contention_conflicts::ContentionConflictManager;
use crate::multiparty::initiate_mp::default_room_id_signing;
use crate::multiparty::watcher::DepositWatcher;
use crate::observability::dynamic_prometheus::update_prometheus_configs;
use crate::shuffle::shuffle_interval::Shuffle;
use crate::util::logging::Loggable;

/**
* Node is the main entry point for the application /
* blockchain node runtime.
* It is responsible for starting all the services and
* Initializing the connection to the network
* managing the lifecycle of the application.
*/
#[derive(Clone)]
pub struct Node {
    pub relay: Relay,
}

impl Node {

    /**
    * Start all background thread application services. REST APIs, event processors, transaction process, etc.
    * Each of these application background services communicates via channels instantiated by the relay
    */
    #[tracing::instrument(skip(relay), fields(node_id = %relay.node_config.public_key().short_id()))]
    pub async fn start_services(relay: Relay) -> Vec<JoinHandle<Result<(), ErrorInfo>>> {

        let node_config = relay.node_config.clone();

        let mut join_handles = vec![
            // Internal RPC control equivalent, used for issuing commands to node
            // Disabled in high security mode
            control_api::ControlServer {
            relay: relay.clone(),
            }.start(),
            // Stream processor for sending external peer messages
            // Negotiates appropriate protocol depending on peer
            PeerOutgoingEventHandler::new(relay.clone()),
            // Main transaction processing loop, watches over lifecycle of a given transaction
            // as it's drawn from the mem-pool
            TransactionProcessContext::new(relay.clone()),
        ];
        // TODO: Filter out any join handles that have terminated immediately with success due to disabled services.

        // TODO: Re-enable auto-update process for installed service as opposed to watchtower docker usage.
        // runtimes
        //     .auxiliary
        //     .spawn(auto_update::from_node_config(node_config.clone()));
        //

        // Components for download now initialized.
        // relay.clone().node_state.store(NodeState::Downloading);


        let ojh = ObservationBuffer::new(relay.clone()).await;
        join_handles.push(ojh);

        // Rewards::new(relay.clone(), runtimes.auxiliary.clone());

        join_handles.push(PeerRxEventHandler::new(
            relay.clone(),
            // runtimes.auxiliary.clone(),
        ));

        join_handles.push(public_api::start_server(relay.clone(),
                                                   // runtimes.public_api.clone()
        ));

        join_handles.push(explorer::server::start_server(relay.clone(),
                                                   // runtimes.public_api.clone()
        ));

        let obs_handler = ObservationHandler{relay: relay.clone()};
        join_handles.push(tokio::spawn(async move { obs_handler.run().await }));
        //
        // let mut mph = MultipartyHandler::new(
        //     relay.clone(),
        //     // runtimes.auxiliary.clone()
        // );
        // join_handles.push(tokio::spawn(async move { mph.run().await }));

        let sm_port = relay.node_config.mparty_port();
        let sm_relay = relay.clone();
        join_handles.push(tokio::spawn(async move { gg20_sm_manager::run_server(sm_port, sm_relay)
                .await.map_err(|e| error_info(e.to_string())) }));


        // let relay_c = relay.clone();
        // let amh = runtimes.async_multi.spawn(async move {
        //     let r = relay_c.clone();
        //     let blocks = BlockFormationProcess::default(r.clone()).await?;
        //     // TODO: Select from list of futures.
        //     Ok::<(), ErrorInfo>(tokio::select! {
        //         res = blocks.run() => {res?}
        //         // res = obs_handler.run() => {res?}
        //         // _ = rosetta::run_server(r.clone()) => {}
        //         // _ = public_api::run_server(r.clone()) => {}
        //     })
        // });
        // join_handles.push(amh);
        let c_config = relay.clone();
        if node_config.e2e_enabled {
            // TODO: Distinguish errors here
            let _cwh = tokio::spawn(e2e::run(c_config));
            // join_handles.push(cwh);
        }

        join_handles.push(update_prometheus_configs(relay.clone()).await);

        let discovery = Discovery::new(relay.clone()).await;
        join_handles.push(stream_handlers::run_interval_fold(
            discovery.clone(), relay.node_config.discovery_interval, false
        ).await);

        join_handles.push(stream_handlers::run_interval_fold(
            DepositWatcher::new(relay.clone()), relay.node_config.watcher_interval, false
        ).await);


        join_handles.push(stream_handlers::run_recv(
            discovery, relay.discovery.receiver.clone(), 100
        ).await);


        join_handles.push(tokio::spawn(api::rosetta::server::run_server(relay.clone())));

        join_handles.push(stream_handlers::run_interval_fold(
            Shuffle::new(&relay), relay.node_config.shuffle_interval, false
        ).await);

        join_handles.push(stream_handlers::run_interval_fold(
            crate::core::mempool::Mempool::new(&relay), relay.node_config.mempool.interval.clone(), false
        ).await);

        for i in 0..relay.node_config.contract.bucket_parallelism {
            let opt_c = relay.contract_state_manager_channels.get(i);
            let c = opt_c.expect("bucket partition creation error");
            let handle = stream_handlers::run_interval_fold_or_recv(
                ContractStateManager::new(relay.clone()),
                relay.node_config.contract.interval.clone(),
                false,
                c.receiver.clone()
            ).await;
            join_handles.push(handle);

            let opt_c = relay.contention.get(i);
            let c = opt_c.expect("bucket partition creation error");
            let handle = stream_handlers::run_interval_fold_or_recv(
                ContentionConflictManager::new(relay.clone()),
                relay.node_config.contention.interval.clone(),
                false,
                c.receiver.clone()
            ).await;
            join_handles.push(handle);


        }

        join_handles.push(stream_handlers::run_interval_fold(
            RecentDownload {
                relay: relay.clone(),
            }, Duration::from_secs(60), false
        ).await);


        // TODO: Change all join handles to a single vec![] instantiation?
        join_handles.push(stream_handlers::run_interval_fold(
            DataDiscovery {
                relay: relay.clone(),
            }, Duration::from_secs(60), false
        ).await);


        join_handles
    }

    pub async fn prelim_setup(
        relay2: Relay,
        // runtimes: NodeRuntimes
    ) -> Result<(), ErrorInfo> {
        let relay = relay2.clone();
        let node_config = relay.node_config.clone();

        relay.ds.run_migrations_fallback_delete(
            node_config.clone().network != NetworkEnvironment::Main,
            node_config.env_data_folder().data_store_path()
        ).await?;
        relay.ds.count_gauges().await?;

        relay.ds.check_consistency_apply_fixes().await?;

        Ok(())
    }

    pub fn throw_error() -> Result<(), ErrorInfo> {
        Err(ErrorInfo::error_info("test"))?;
        Ok(())
    }

    pub fn throw_error_panic() -> Result<(), ErrorInfo> {
        let result3: Result<Node, ErrorInfo> = Err(ErrorInfo::error_info("test"));
        result3.expect("expected panic");
        Ok(())
    }

    pub fn genesis_from(node_config: NodeConfig) -> (Transaction, Vec<SpendableUTXO>) {
        let tx = genesis_transaction(&node_config.network, &node_config.words(), &node_config.seeds);
        let outputs = tx.utxo_outputs().expect("utxos");
        let mut res = vec![];
        for i in 0..50 {
            let kp = node_config.words().keypair_at_change(i).expect("works");
            let address = kp.address_typed();
            let o = outputs.iter().find(|o| {
                address == o.address().as_ref().expect("a").clone().clone()
            }).expect("found");
            let s = SpendableUTXO {
                utxo_entry: o.clone(),
                key_pair: kp,
            };
            res.push(s);
        }
        (tx, res)
    }

    pub async fn from_config(relay: Relay) -> Result<Node, ErrorInfo> {

        let node = Self {
            relay: relay.clone()
        };

        let node_config = relay.node_config.clone();

        relay.update_nmd_auto().await?;

        if node_config.genesis {
            info!("Starting from genesis");
            // relay.node_state.store(NodeState::Ready);
            // TODO: Replace with genesis per network type.
            // if node_config.is_debug() {
            //     info!("Genesis code kp");
            //     let _res_err = DataStore::map_err(
            //         relay
            //             .ds
            //             .insert_transaction(&create_genesis_transaction(), EARLIEST_TIME),
            //     );
            // } else {
            //     info!("Genesis local test multiple kp");

            let existing = relay.ds.config_store.get_maybe_proto::<Transaction>("genesis").await?;

            if existing.is_none() {
                info!("No genesis transaction found, generating new one");
                let tx = genesis_transaction(&node_config.network, &node_config.words(), &node_config.seeds);
                // let tx = Node::genesis_from(node_config.clone()).0;
                // runtimes.auxiliary.block_on(
                relay.ds.config_store.store_proto("genesis", tx.clone()).await?;
                let _res_err =
                    // runtimes.auxiliary.block_on(
                    relay
                        .ds
                        .transaction_store
                        .insert_transaction(&tx.clone(), EARLIEST_TIME, true, None, true)
                        .await.expect("insert failed");
                // }
                let genesis_hash = tx.hash_or();
                info!("Genesis hash {}", genesis_hash.json_or());
                let obs = relay.observe_tx(&genesis_hash, State::Pending, ValidationType::Full, structs::ValidationLiveness::Live).await?;
                let obs = relay.observe_tx(&genesis_hash, State::Accepted, ValidationType::Full, structs::ValidationLiveness::Live).await?;
                assert_eq!(relay.ds.observation.select_observation_edge(&genesis_hash).await?.len(), 2);
                // .expect("Genesis inserted or already exists");
            }

        } else {

            info!("Starting from seed nodes");
            let seed = if node_config.main_stage_network() {
                info!("Querying LB for node info");
                let a =
                    // runtimes.auxiliary.block_on(
                    node_config.api_client().about().await?;
                    // )?;
                let tx = a.latest_metadata.safe_get_msg("Missing latest metadata from seed node")?;
                let pd = tx.outputs.get(0).expect("a").data.as_ref().expect("d").peer_data.as_ref().expect("pd");
                let nmd = pd.node_metadata.get(0).expect("nmd");
                let _vec = nmd.public_key_bytes().expect("ok");
                let vec1 = pd.peer_id.safe_get()?.clone().peer_id.safe_bytes()?.clone();
                // TODO: Derive from NodeMetadata?
                Seed{
                    peer_id: Some(PeerId::from_bytes(vec1)),
                    trust: vec![TrustData::from_label(1.0)],
                    public_key: Some(nmd.public_key.safe_get_msg("Missing pk on about").cloned()?),
                    external_address: nmd.external_address()?.clone(),
                    port_offset: Some(nmd.port_or(node_config.network) as u32),
                    environments: vec![node_config.network as i32],
                }
            } else {
                relay.node_config.seeds.get(0).unwrap().clone()

            };

            // Change to immediate discovery message
            let port = seed.port_offset.unwrap() + 1;
            let client = PublicClient::from(seed.external_address.clone(), port as u16, Some(relay.clone()));
            info!("Querying with public client for node info again on: {} : {:?}", seed.external_address, port);
            let response = client.about().await?;
            let result = response.peer_node_info.safe_get()?;

            info!("Got LB node info {}, adding peer", result.json_or());

            // TODO: How do we handle situation where we get self peer id here other than an error?
            relay.ds.peer_store.add_peer_new(result, &relay.node_config.public_key()).await?;

            info!("Added peer, attempting download");

            let x = result.latest_node_transaction.safe_get()?;
            let metadata = x.node_metadata()?;
            let pk1 = metadata.public_key.safe_get()?;

            // ensure peer added successfully
            let key = pk1.clone();
            let qry_result = relay.ds.peer_store.query_public_key_node(&key).await;
            qry_result.log_error().ok();
            let opt_info = qry_result.expect("query public key node");
            let pk_store = opt_info
                .expect("query public key node")
                .node_metadata().expect("node metadata")
                .public_key.expect("pk");
            assert_eq!(&pk_store, pk1);

            // This triggers peer exploration.
            tracing::info!("Attempting discovery process of peers on startup for node");
            let mut discovery = Discovery::new(relay.clone()).await;
            discovery.interval_fold().await?;
            // This was only immediate discovery of that node and is already covered
            // // TODO: Remove this in favor of discovery
            // relay.discovery.sender.send_err(
            //     DiscoveryMessage::new(metadata.clone(), result.dynamic_node_metadata.clone())
            // )?;



            tokio::time::sleep(Duration::from_secs(3)).await;
            info!("Now starting download after discovery has ran.");

            // TODO Change this invocation to an .into() in a non-schema key module
            download::download(
                relay.clone(),
                pk1.clone()
            ).await;


        }

        info!("Node ready");
        counter!("redgold.node.node_started").increment(1);

        return Ok(node);
    }
}
