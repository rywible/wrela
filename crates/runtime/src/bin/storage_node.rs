#[cfg(feature = "test-utils")]
mod test_utils {
    use std::collections::HashMap;
    use std::env;
    use std::io::{self, Write};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use tokio::time::sleep;

    use wrela_runtime::storage::config::StorageConfig;
    use wrela_runtime::storage::service::{StorageRequest, StorageService};
    use wrela_runtime::storage::set_drop_replication;
    use wrela_runtime::storage::store::NodeId;
    use openraft::BasicNode;

    pub async fn run() -> Result<(), String> {
        let args = Args::parse(env::args().collect())?;

        let drop_replication = env::var("WRELA_TEST_DROP_REPLICATION").is_ok();

        let config = StorageConfig {
            enabled: true,
            path: args.path.clone(),
            node_id: args.node_id,
            bind_addr: args.bind_addr.clone(),
            http_enabled: true,
            peers: args.peers.clone(),
            bootstrap: args.bootstrap,
            snapshot_interval: args.snapshot_interval,
            batch_max_ops: 2,
            batch_max_ms: 1,
            queue_cap: 64,
        };

        let service = StorageService::start_for_test(config)
            .await
            .map_err(|err| err.to_string())?;
        let service = Arc::new(service);

        log_line(&format!("READY {}", args.node_id));

        if args.init_cluster {
            if args.bootstrap {
                wait_for_leader(&service).await?;
            } else {
                for (id, addr) in args.peers.iter() {
                    add_learner_with_retry(&service, *id, addr.clone()).await?;
                }

                let mut members = Vec::with_capacity(args.peers.len() + 1);
                members.push(args.node_id);
                members.extend(args.peers.keys().cloned());
                change_membership_with_retry(&service, members).await?;
                wait_for_leader(&service).await?;
            }

            if drop_replication && !args.peers.is_empty() {
                wait_for_membership_replication(&service, &args.peers).await?;
                set_drop_replication(true);
            } else if drop_replication {
                set_drop_replication(true);
            }

            log_line("CLUSTER_READY");
        }

        if let Some((key, value)) = args.write {
            log_line("WRITE_STARTED");
            let write_service = Arc::clone(&service);
            tokio::spawn(async move {
                let resp = write_service
                    .dispatch_to(StorageRequest::Put { key, value })
                    .await;
                match resp {
                    Ok(_) => log_line("WRITE_OK"),
                    Err(err) => log_line(&format!("WRITE_ERR {}", err)),
                }
            });
        }

        loop {
            sleep(Duration::from_secs(3600)).await;
        }
    }

    fn log_line(line: &str) {
        println!("{line}");
        let _ = io::stdout().flush();
    }

    async fn wait_for_leader(service: &StorageService) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if service.raft_ref().metrics().borrow().current_leader.is_some() {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err("timed out waiting for leader".to_string());
            }
            sleep(Duration::from_millis(50)).await;
        }
    }

    async fn wait_for_membership_replication(
        service: &StorageService,
        peers: &HashMap<NodeId, String>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let metrics = service.raft_ref().metrics().borrow().clone();
            let membership_log_id = metrics.membership_config.log_id();
            if let (Some(log_id), Some(replication)) = (membership_log_id, metrics.replication) {
                let mut all_replicated = true;
                for id in peers.keys() {
                    match replication.get(id).and_then(|v| *v) {
                        Some(rep_log_id) if rep_log_id >= *log_id => {}
                        _ => {
                            all_replicated = false;
                            break;
                        }
                    }
                }
                if all_replicated {
                    return Ok(());
                }
            }
            if Instant::now() > deadline {
                return Err("timed out waiting for membership replication".to_string());
            }
            sleep(Duration::from_millis(50)).await;
        }
    }

    async fn add_learner_with_retry(
        service: &StorageService,
        node_id: NodeId,
        addr: String,
    ) -> Result<(), String> {
        let mut tries = 0u32;
        loop {
            match service
                .raft_ref()
                .add_learner(node_id, BasicNode { addr: addr.clone() }, true)
                .await
            {
                Ok(_) => return Ok(()),
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("InProgress") || msg.contains("configuration change") {
                        tries += 1;
                        if tries > 200 {
                            return Err(err.to_string());
                        }
                        sleep(Duration::from_millis(25)).await;
                        continue;
                    }
                    return Err(err.to_string());
                }
            }
        }
    }

    async fn change_membership_with_retry(
        service: &StorageService,
        members: Vec<NodeId>,
    ) -> Result<(), String> {
        let mut tries = 0u32;
        loop {
            match service.raft_ref().change_membership(members.clone(), false).await {
                Ok(_) => return Ok(()),
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("InProgress") || msg.contains("configuration change") {
                        tries += 1;
                        if tries > 200 {
                            return Err(err.to_string());
                        }
                        sleep(Duration::from_millis(25)).await;
                        continue;
                    }
                    return Err(err.to_string());
                }
            }
        }
    }

    struct Args {
        node_id: NodeId,
        bind_addr: String,
        path: String,
        peers: HashMap<NodeId, String>,
        bootstrap: bool,
        snapshot_interval: u64,
        init_cluster: bool,
        write: Option<(Vec<u8>, Vec<u8>)>,
    }

    impl Args {
        fn parse(args: Vec<String>) -> Result<Self, String> {
            let mut node_id = None;
            let mut bind_addr = None;
            let mut path = None;
            let mut peers = HashMap::new();
            let mut bootstrap = false;
            let mut snapshot_interval = 50u64;
            let mut init_cluster = false;
            let mut write = None;

            let mut it = args.into_iter().skip(1);
            while let Some(arg) = it.next() {
                match arg.as_str() {
                    "--node-id" => {
                        let val = it.next().ok_or("missing --node-id")?;
                        node_id = Some(val.parse::<NodeId>().map_err(|_| "bad node id")?);
                    }
                    "--bind-addr" => {
                        bind_addr = Some(it.next().ok_or("missing --bind-addr")?);
                    }
                    "--path" => {
                        path = Some(it.next().ok_or("missing --path")?);
                    }
                    "--peer" => {
                        let val = it.next().ok_or("missing --peer")?;
                        let (id_str, addr) = val.split_once('=').ok_or("bad --peer")?;
                        let id = id_str.parse::<NodeId>().map_err(|_| "bad peer id")?;
                        peers.insert(id, addr.to_string());
                    }
                    "--bootstrap" => {
                        let val = it.next().ok_or("missing --bootstrap")?;
                        bootstrap = matches!(val.as_str(), "1" | "true" | "yes");
                    }
                    "--snapshot-interval" => {
                        let val = it.next().ok_or("missing --snapshot-interval")?;
                        snapshot_interval = val.parse::<u64>().map_err(|_| "bad snapshot")?;
                    }
                    "--init-cluster" => init_cluster = true,
                    "--write" => {
                        let key = it.next().ok_or("missing --write key")?;
                        let value = it.next().ok_or("missing --write value")?;
                        write = Some((key.into_bytes(), value.into_bytes()));
                    }
                    _ => return Err(format!("unknown arg: {arg}")),
                }
            }

            Ok(Self {
                node_id: node_id.ok_or("missing --node-id")?,
                bind_addr: bind_addr.ok_or("missing --bind-addr")?,
                path: path.ok_or("missing --path")?,
                peers,
                bootstrap,
                snapshot_interval,
                init_cluster,
                write,
            })
        }
    }
}

#[cfg(feature = "test-utils")]
#[tokio::main]
async fn main() {
    if let Err(err) = test_utils::run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

#[cfg(not(feature = "test-utils"))]
fn main() {
    eprintln!("storage_node requires the test-utils feature");
    std::process::exit(1);
}
