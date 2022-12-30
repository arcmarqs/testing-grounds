use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use chrono::offset::Utc;
use futures_timer::Delay;
use intmap::IntMap;
use nolock::queues::mpsc::jiffy::{
    async_queue,
    AsyncSender,
};
use rand_core::{OsRng, RngCore};

use febft::bft::{
    init,
    InitConfig, prng,
};
use febft::bft::async_runtime as rt;
use febft::bft::benchmarks::{BenchmarkHelper, BenchmarkHelperStore, CommStats};
use febft::bft::communication::{channel, PeerAddr};
use febft::bft::communication::NodeId;
use febft::bft::core::client::Client;
use febft::bft::core::client::ordered_client::Ordered;
use febft::bft::crypto::signature::{
    KeyPair,
    PublicKey,
};

use crate::common::*;
use crate::serialize::{CalcData, Action};

pub fn main() {
    let is_client = std::env::var("CLIENT")
        .map(|x| x == "1")
        .unwrap_or(false);

    let single_server = std::env::var("SINGLE_SERVER")
    .map(|x| x == "1")
    .unwrap_or(false);

    let conf = InitConfig {
        threadpool_threads: 5,
        async_threads: num_cpus::get() / 1,
        id: None
    };

    let _guard = unsafe { init(conf).unwrap() };

    println!("Starting...");

    if !is_client {
        if !single_server {
        main_();
        } else {
            run_single_server();            
        }
    } else {
        rt::block_on(client_async_main());
    }
}

fn main_() {
    let clients_config = parse_config("./config/clients.config").unwrap();
    let replicas_config = parse_config("./config/replicas.config").unwrap();

    println!("Read configurations.");

    let mut secret_keys: IntMap<KeyPair> = sk_stream()
        .take(replicas_config.len())
        .enumerate()
        .map(|(id, sk)| (id as u64, sk))
        .collect();
    let public_keys: IntMap<PublicKey> = secret_keys
        .iter()
        .map(|(id, sk)| (*id, sk.public_key().into()))
        .collect();

    println!("Read keys.");

    let mut pending_threads = Vec::with_capacity(4);

    let first_cli = NodeId::from(1000u32);

    for replica in &replicas_config {
        let id = NodeId::from(replica.id);

        println!("Starting replica {:?}", id);

        let addrs = {
            let mut addrs = IntMap::new();

            for other in &replicas_config {
                let id = NodeId::from(other.id);
                let addr = format!("{}:{}", other.ipaddr, other.portno);
                let replica_addr = format!("{}:{}", other.ipaddr, other.rep_portno.unwrap());

                let client_addr = PeerAddr::new_replica(crate::addr!(&other.hostname => addr),
                                                        crate::addr!(&other.hostname => replica_addr));

                addrs.insert(id.into(), client_addr);
            }

            for client in &clients_config {
                let id = NodeId::from(client.id);
                let addr = format!("{}:{}", client.ipaddr, client.portno);

                let replica = PeerAddr::new(crate::addr!(&client.hostname => addr));

                addrs.insert(id.into(), replica);
            }

            addrs
        };

        let sk = secret_keys.remove(id.into()).unwrap();

        let comm_stats = Arc::new(CommStats::new(id,
                                                 first_cli,
                                                 10000));

        println!("Setting up replica...");
        let fut = setup_replica(
            replicas_config.len(),
            id,
            sk,
            addrs,
            public_keys.clone(),
            Some(comm_stats)
        );

        pending_threads.push(std::thread::spawn(move || {
            let mut replica = rt::block_on(async move {
                println!("Bootstrapping replica #{}", u32::from(id));
                let replica = fut.await.unwrap();
                println!("Running replica #{}", u32::from(id));
                replica
            });

            replica.run().unwrap();
        }));
    }

    //We will only launch a single OS monitoring thread since all replicas also run on the same system
   // crate::os_statistics::start_statistics_thread(NodeId(0));

    drop((secret_keys, public_keys, clients_config, replicas_config));

    // run forever
    for x in pending_threads {
        x.join();
    }
}

fn run_single_server() {
    let clients_config = parse_config("./config/clients.config").unwrap();
    let replicas_config = parse_config("./config/replicas.config").unwrap();

    println!("Read configurations.");

    let mut secret_keys: IntMap<KeyPair> = sk_stream()
        .take(replicas_config.len())
        .enumerate()
        .map(|(id, sk)| (id as u64, sk))
        .collect();
    let public_keys: IntMap<PublicKey> = secret_keys
        .iter()
        .map(|(id, sk)| (*id, sk.public_key().into()))
        .collect();

    println!("Read keys.");

    let first_cli = NodeId::from(1000u32);
    let replica_id: usize = std::env::args()
    .nth(1).expect("No replica specified")
    .trim().parse().expect("Expected an integer");

    let replica = &replicas_config[replica_id];

    let id = NodeId::from(replica.id);

    println!("Starting replica {:?}", id);

    let addrs = {
        let mut addrs = IntMap::new();

        for other in &replicas_config {
            let id = NodeId::from(other.id);
            let addr = format!("{}:{}", other.ipaddr, other.portno);
            let replica_addr = format!("{}:{}", other.ipaddr, other.rep_portno.unwrap());

            let client_addr = PeerAddr::new_replica(crate::addr!(&other.hostname => addr),
                                                    crate::addr!(&other.hostname => replica_addr));

            addrs.insert(id.into(), client_addr);
        }

        for client in &clients_config {
            let id = NodeId::from(client.id);
            let addr = format!("{}:{}", client.ipaddr, client.portno);

            let replica = PeerAddr::new(crate::addr!(&client.hostname => addr));

            addrs.insert(id.into(), replica);
        }

        addrs
    };

    let sk = secret_keys.remove(id.into()).unwrap();

    let comm_stats = Arc::new(CommStats::new(id,
                                                first_cli,
                                                10000));

    println!("Setting up replica...");
    let fut = setup_replica(
        replicas_config.len(),
        id,
        sk,
        addrs,
        public_keys.clone(),
        Some(comm_stats)
    );

        let mut replica = rt::block_on(async move {
            println!("Bootstrapping replica #{}", u32::from(id));
            let replica = fut.await.unwrap();
            println!("Running replica #{}", u32::from(id));
            replica
        });
        
        replica.run().unwrap();
    //We will only launch a single OS monitoring thread since all replicas also run on the same system
   // crate::os_statistics::start_statistics_thread(NodeId(0));

    drop((secret_keys, public_keys, clients_config, replicas_config));
}

async fn client_async_main() {
    let clients_config = parse_config("./config/clients.config").unwrap();
    let replicas_config = parse_config("./config/replicas.config").unwrap();

    let mut secret_keys: IntMap<KeyPair> = sk_stream()
        .take(clients_config.len())
        .enumerate()
        .map(|(id, sk)| (1000 + id as u64, sk))
        .chain(sk_stream()
            .take(replicas_config.len())
            .enumerate()
            .map(|(id, sk)| (id as u64, sk)))
        .collect();
    let public_keys: IntMap<PublicKey> = secret_keys
        .iter()
        .map(|(id, sk)| (*id, sk.public_key().into()))
        .collect();

    let (tx, mut rx) = channel::new_bounded_async(clients_config.len());

    let mut first_cli : u32 = u32::MAX;


    for client in &clients_config {
        let _id = NodeId::from(client.id);

        if client.id < first_cli {
            first_cli = client.id;
        }
    }

    let comm_stats = Arc::new(CommStats::new(NodeId::from(first_cli),
                                             NodeId::from(first_cli),
                                             10000));

    for client in &clients_config {
        let id = NodeId::from(client.id);

        if client.id < first_cli {
            first_cli = client.id;
        }

        let addrs = {
            let mut addrs = IntMap::new();

            for replica in &replicas_config {
                let id = NodeId::from(replica.id);
                let addr = format!("{}:{}", replica.ipaddr, replica.portno);
                let replica_addr = format!("{}:{}", replica.ipaddr, replica.rep_portno.unwrap());

                let replica_p_addr = PeerAddr::new_replica(crate::addr!(&replica.hostname => addr),
                                                           crate::addr!(&replica.hostname => replica_addr));

                addrs.insert(id.into(), replica_p_addr);
            }

            for other in &clients_config {
                let id = NodeId::from(other.id);
                let addr = format!("{}:{}", other.ipaddr, other.portno);

                let client_addr = PeerAddr::new(crate::addr!(&other.hostname => addr));

                addrs.insert(id.into(), client_addr);
            }

            addrs
        };

        let sk = secret_keys.remove(id.into()).unwrap();

        let fut = setup_client(
            replicas_config.len(),
            id,
            sk,
            addrs,
            public_keys.clone(),
            Some(comm_stats.clone())
        );

        let mut tx = tx.clone();

        rt::spawn(async move {
            println!("Bootstrapping client #{}", u32::from(id));
            let client = fut.await.unwrap();
            println!("Done bootstrapping client #{}", u32::from(id));
            tx.send(client).await.unwrap();
        });
    }

    drop((secret_keys, public_keys, replicas_config));

    let (mut queue, queue_tx) = async_queue();
    let queue_tx = Arc::new(queue_tx);

    let mut clients = Vec::with_capacity(clients_config.len());

    for _i in 0..clients_config.len() {
        clients.push(rx.recv().await.unwrap());
    }

    //We have all the clients, start the OS resource monitoring thread
   // crate::os_statistics::start_statistics_thread(NodeId(first_cli));

    let mut handles = Vec::with_capacity(clients_config.len());

    for client in clients {
        let queue_tx = Arc::clone(&queue_tx);

        let h = std::thread::spawn(move || {
            run_client(client, queue_tx)
        });

        handles.push(h);
    }

    drop(clients_config);

    for h in handles {
        let _ = h.join();
    }

    let mut file = File::create("./latencies.out").unwrap();

    while let Ok(line) = queue.try_dequeue() {
        file.write_all(line.as_ref()).unwrap();
    }

    file.flush().unwrap();
}

fn sk_stream() -> impl Iterator<Item=KeyPair> {
    std::iter::repeat_with(|| {
        // only valid for ed25519!
        let buf = [0; 32];
        KeyPair::from_bytes(&buf[..]).unwrap()
    })
}

fn run_client(mut client: Client<CalcData>, _q: Arc<AsyncSender<String>>) {


    println!("Warm up...");

    let _id = u32::from(client.id());

    for _ in 0..4096 {
        let mut rng = prng::State::new();
        let request = {
            let i = rng.next_state();
            if i & 1 == 0 { Action::Sqrt } else { Action::MultiplyByTwo }
        };
        println!("{:?} // Sending req {:?}...", client.id(), request);

        if let Ok(reply) = rt::block_on(client.update::<Ordered>(Arc::from(request))) {
            println!("state: {:?}", reply);
        }
     
    }
}