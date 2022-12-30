use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::iter;
use std::sync::{Arc};

use febft::bft::executable::Service;
use intmap::IntMap;
use konst::primitive::{parse_u128, parse_usize};
use konst::unwrap_ctx;
use regex::Regex;
use rustls::{Certificate, ClientConfig, PrivateKey, RootCertStore, ServerConfig};
use rustls::server::AllowAnyAuthenticatedClient;
use rustls_pemfile::{read_one, Item};
use febft::bft::benchmarks::CommStats;

use febft::bft::communication::{NodeConfig, NodeId, PeerAddr};
use febft::bft::consensus::log::persistent::{NoPersistentLog};
use febft::bft::core::client::{
    self,
    Client,
};
use febft::bft::core::client::unordered_client::UnorderedClientMode;
use febft::bft::core::server::{
    Replica,
    ReplicaConfig,
};
use febft::bft::crypto::signature::{
    KeyPair,
    PublicKey,
};
use febft::bft::error::*;
use febft::bft::ordering::SeqNo;
use febft::bft::threadpool;

use crate::serialize::{Action, CalcData};


#[macro_export]
macro_rules! addr {
    ($h:expr => $a:expr) => {{
        let addr: ::std::net::SocketAddr = $a.parse().unwrap();
        (addr, String::from($h))
    }}
}

pub struct ConfigEntry {
    pub portno: u16,
    pub rep_portno: Option<u16>,
    pub id: u32,
    pub hostname: String,
    pub ipaddr: String,
}

pub fn parse_config(path: &str) -> Option<Vec<ConfigEntry>> {
    let re = Regex::new("([^ ]+)").ok()?;

    let file = File::open(path).ok()?;
    let mut file = BufReader::new(file);

    let mut buf = String::new();
    let mut config = Vec::new();

    loop {
        match file.read_line(&mut buf) {
            Ok(0) | Err(_) => break,
            _ => {
                match parse_entry(&re, &buf) {
                    Some(entry) => config.push(entry),
                    None => (),
                }
                buf.clear();
            }
        }
    }

    Some(config)
}

fn parse_entry(re: &Regex, line: &str) -> Option<ConfigEntry> {
    let line = line.trim();

    if line.chars().next() == Some('#') {
        return None;
    }

    let matches: Vec<_> = re
        .find_iter(line)
        .collect();

    if matches.len() < 4 {
        return None;
    }

    let id: u32 = matches[0].as_str().parse().ok()?;
    let hostname: String = matches[1].as_str().to_string();
    let ipaddr: String = matches[2].as_str().to_string();
    let portno: u16 = matches[3].as_str().trim_end().parse().ok()?;

    let rep_portno: Option<u16> =
        if matches.len() >= 5 {
            Some(matches[4].as_str().trim_end().parse().ok()?)
        } else { None };

    Some(ConfigEntry { id, rep_portno, hostname, ipaddr, portno })
}

async fn node_config(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> NodeConfig {

    let db_path = format!("PERSISTENT_DB_{:?}", id);

    // read TLS configs concurrently
    let (client_config, server_config, client_config_replica, server_config_replica, batch_size,
        batch_timeout, batch_sleep, clients_per_pool) = {
        let cli = get_client_config(id);
        let srv = get_server_config(id);
        let cli_rustls = get_client_config_replica(id);
        let srv_rustls = get_server_config_replica(id);
        let batch_size = get_batch_size();
        let batch_timeout = get_batch_timeout();
        let batch_sleep = get_batch_sleep();
        let clients_per_pool = get_clients_per_pool();
        futures::join!(cli, srv, cli_rustls, srv_rustls, batch_size,
            batch_timeout, batch_sleep, clients_per_pool)
    };

    // build the node conf
    NodeConfig {
        id,
        n,
        f: (n - 1) / 3,
        sk,
        pk,
        addrs,
        async_client_config: client_config,
        async_server_config: server_config,
        sync_client_config: client_config_replica,
        sync_server_config: server_config_replica,
        first_cli: NodeId::from(1000u32),
        batch_size,
        clients_per_pool,
        batch_timeout_micros: batch_timeout as u64,
        batch_sleep_micros: batch_sleep as u64,
        comm_stats,
        db_path,
    }
}

pub async fn setup_client(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> Result<Client<CalcData>> {
    let node = node_config(n, id, sk, addrs, pk, comm_stats).await;
    let conf = client::ClientConfig {
        unordered_rq_mode: UnorderedClientMode::BFT,
        node,
    };
    Client::bootstrap(conf).await
}

pub async fn setup_replica(
    n: usize,
    id: NodeId,
    sk: KeyPair,
    addrs: IntMap<PeerAddr>,
    pk: IntMap<PublicKey>,
    comm_stats: Option<Arc<CommStats>>,
) -> Result<Replica<CalcService, NoPersistentLog>> {
    let node_id = id.clone();

    let (node, global_batch_size, global_batch_timeout) = {
        let n = node_config(n, id, sk, addrs, pk, comm_stats);
        let b = get_global_batch_size();
        let timeout = get_global_batch_timeout();
        futures::join!(n, b, timeout)
    };

    let conf = ReplicaConfig {
        node,
        global_batch_size,
        view: SeqNo::ZERO,
        next_consensus_seq: SeqNo::ZERO,
        service: CalcService{
            0:id,
            1:0,
        },
        batch_timeout: global_batch_timeout,
        log_mode: Default::default(),
    };

    Replica::bootstrap(conf).await
}

async fn get_batch_size() -> usize {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let mut buf = String::new();
        let mut f = open_file("./config/batch.config");
        f.read_to_string(&mut buf).unwrap();
        tx.send(buf.trim().parse().unwrap()).unwrap();
    });
    rx.await.unwrap()
}

async fn get_global_batch_size() -> usize {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_usize(&*std::env::var("GLOBAL_BATCH_SIZE")
            .expect("Failed to find required env var GLOBAL_BATCH_SIZE"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_global_batch_timeout() -> u128 {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_u128(&*std::env::var("GLOBAL_BATCH_SLEEP_MICROS")
            .expect("Failed to find required env var GLOBAL_BATCH_SLEEP_MICROS"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_batch_timeout() -> u128 {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_u128(&*std::env::var("BATCH_TIMEOUT_MICROS")
            .expect("Failed to find required env var BATCH_TIMEOUT_MICROS"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_batch_sleep() -> u128 {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_u128(&*std::env::var("BATCH_SLEEP_MICROS")
            .expect("Failed to find required env var BATCH_SLEEP_MICROS"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

async fn get_clients_per_pool() -> usize {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let res = parse_usize(&*std::env::var("CLIENTS_PER_POOL")
            .expect("Failed to find required env var CLIENTS_PER_POOL"));

        tx.send(unwrap_ctx!(res)).expect("Failed to send");
    });
    rx.await.unwrap()
}

fn read_certificates_from_file(mut file: &mut BufReader<File>) -> Vec<Certificate> {
    let mut certs = Vec::new();

    for item in iter::from_fn(|| read_one(&mut file).transpose()) {
        match item.unwrap() {
            Item::X509Certificate(cert) => {
                certs.push(Certificate(cert));
            }
            Item::RSAKey(_) => {
                panic!("Key given in place of a certificate")
            }
            Item::PKCS8Key(_) => {
                panic!("Key given in place of a certificate")
            }
            Item::ECKey(_) => {
                panic!("Key given in place of a certificate")
            }
            _ => {
                panic!("Key given in place of a certificate")
            }
        }
    }

    certs
}

#[inline]
fn read_private_keys_from_file(mut file: BufReader<File>) -> Vec<PrivateKey> {
    let mut certs = Vec::new();

    for item in iter::from_fn(|| read_one(&mut file).transpose()) {
        match item.unwrap() {
            Item::RSAKey(rsa) => {
                certs.push(PrivateKey(rsa))
            }
            Item::PKCS8Key(rsa) => {
                certs.push(PrivateKey(rsa))
            }
            Item::ECKey(rsa) => {
                certs.push(PrivateKey(rsa))
            }
            _ => {
                panic!("Key given in place of a certificate")
            }
        }
    }

    certs
}

fn read_private_key_from_file(mut file: BufReader<File>) -> PrivateKey {
    read_private_keys_from_file(file).pop().unwrap()
}

async fn get_server_config(id: NodeId) -> ServerConfig {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let id = usize::from(id);
        let mut root_store = RootCertStore::empty();

        // read ca file
        let cert = {
            let mut file = open_file("./ca-root/crt");

            let certs = read_certificates_from_file(&mut file);

            root_store.add(&certs[0]).expect("Failed to put root store");

            certs
        };

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };

        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };

            let mut certs = read_certificates_from_file(&mut file);

            certs.extend(cert);
            certs
        };

        // create server conf
        let auth = AllowAnyAuthenticatedClient::new(root_store);
        let cfg = ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(auth)
            .with_single_cert(chain, sk)
            .expect("Failed to make cfg");

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_server_config_replica(id: NodeId) -> rustls::ServerConfig {
    let (tx, rx) = oneshot::channel();

    threadpool::execute(move || {
        let id = usize::from(id);
        let mut root_store = RootCertStore::empty();

        // read ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");

            read_certificates_from_file(&mut file)
        };

        root_store.add(&certs[0]).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };
        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };

            let mut c = read_certificates_from_file(&mut file);

            c.extend(certs);

            c
        };

        // create server conf
        let auth = AllowAnyAuthenticatedClient::new(root_store);

        let cfg = ServerConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_client_cert_verifier(auth)
            .with_single_cert(chain, sk)
            .expect("Failed to make cfg");

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_client_config(id: NodeId) -> ClientConfig {
    let (tx, rx) = oneshot::channel();
    threadpool::execute(move || {
        let id = usize::from(id);

        let mut root_store = RootCertStore::empty();

        // configure ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            read_certificates_from_file(&mut file)
        };

        root_store.add(&certs[0]).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };

        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = read_certificates_from_file(&mut file);

            c.extend(certs);
            c
        };

        let cfg = ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(root_store)
            .with_single_cert(chain, sk)
            .expect("bad cert/key");

        tx.send(cfg).unwrap();
    });
    rx.await.unwrap()
}

async fn get_client_config_replica(id: NodeId) -> rustls::ClientConfig {
    let (tx, rx) = oneshot::channel();

    threadpool::execute(move || {
        let id = usize::from(id);

        let mut root_store = RootCertStore::empty();

        // configure ca file
        let certs = {
            let mut file = open_file("./ca-root/crt");
            read_certificates_from_file(&mut file)
        };

        root_store.add(&certs[0]).unwrap();

        // configure our cert chain and secret key
        let sk = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/key", id))
            } else {
                open_file(&format!("./ca-root/cli{}/key", id))
            };

            read_private_key_from_file(file)
        };

        let chain = {
            let mut file = if id < 1000 {
                open_file(&format!("./ca-root/srv{}/crt", id))
            } else {
                open_file(&format!("./ca-root/cli{}/crt", id))
            };
            let mut c = read_certificates_from_file(&mut file);

            c.extend(certs);
            c
        };

        let cfg = ClientConfig::builder()
            .with_safe_default_cipher_suites()
            .with_safe_default_kx_groups()
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(root_store)
            .with_single_cert(chain, sk)
            .expect("bad cert/key");

        tx.send(cfg).unwrap();
    });

    rx.await.unwrap()
}


fn open_file(path: &str) -> BufReader<File> {
    let file = File::open(path).expect(path);
    BufReader::new(file)
}

pub struct CalcService(NodeId, i32);

impl Service for CalcService {
    type Data = CalcData;

    fn initial_state(&mut self) -> Result<f32> {
        Ok(1.0)
    }

    fn update(&mut self, state: &mut f32, request: Arc<Action>) -> Arc<f32> {
            match request.as_ref() {
            Action::Sqrt => *state = state.sqrt(),
            Action::MultiplyByTwo => *state *= 2.0,
            Action::NoOp => println!("NoOp"),
            }
        
        let id = u32::from(self.0);
        println!("{:08}: state on replica #{}: {:?}", self.1, id, *state);
        self.1 += 1;
        let reply = Arc::new(*state);

        reply
    }

    fn unordered_execution(&self, state: &febft::bft::executable::State<Self>, request: febft::bft::executable::Request<Self>) -> febft::bft::executable::Reply<Self> {
        todo!()
    }

    fn unordered_batched_execution(
        &self,
        state: &febft::bft::executable::State<Self>,
        requests: febft::bft::executable::UnorderedBatch<febft::bft::executable::Request<Self>>,
    ) -> febft::bft::executable::BatchReplies<febft::bft::executable::Reply<Self>> {
        let mut reply_batch = febft::bft::executable::BatchReplies::with_capacity(requests.len());

        for unordered_req in requests.into_inner() {
            let (peer_id, sess, opid, req) = unordered_req.into_inner();
            let reply = self.unordered_execution(&state, req);
            reply_batch.add(peer_id, sess, opid, reply);
        }

        reply_batch
    }

    fn update_batch(
        &mut self,
        state: &mut febft::bft::executable::State<Self>,
        batch: febft::bft::executable::UpdateBatch<febft::bft::executable::Request<Self>>,
        _meta: febft::bft::benchmarks::BatchMeta,
    ) -> febft::bft::executable::BatchReplies<febft::bft::executable::Reply<Self>> {
        let mut reply_batch = febft::bft::executable::BatchReplies::with_capacity(batch.len());

        for update in batch.into_inner() {
            let (peer_id, sess, opid, req) = update.into_inner();
            let reply = self.update(state, req);
            reply_batch.add(peer_id, sess, opid, reply);
        }

        reply_batch
    }
}
