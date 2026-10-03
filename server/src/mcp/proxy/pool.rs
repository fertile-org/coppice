//! Shared plugin MCP server instances, one per `(plugin, server)` across runs.

use super::transport::{McpConnection, McpTransport, ProxyError, RemoteTool, Transports};
use crate::mcp::protocol::{ToolContent, ToolResult};
use crate::plugins::capability::McpServerEntry;
use crate::plugins::placeholders::{resolve, server_env_allowed, ResolveCtx, ResolvedTransport};
use coppice_config::PluginsConfig;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tokio::time::Instant;
use uuid::Uuid;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct ServerKey {
    pub plugin_id: Uuid,
    pub server: String,
}

#[derive(Clone)]
pub struct PoolServerSpec {
    pub key: ServerKey,
    pub plugin_name: String,
    pub plugin_root: PathBuf,
    pub entry: McpServerEntry,
    pub settings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerHealth {
    Stopped,
    Starting,
    Ready,
    Backoff,
    Unhealthy,
}

impl ServerHealth {
    pub fn as_str(&self) -> &'static str {
        match self {
            ServerHealth::Stopped => "stopped",
            ServerHealth::Starting => "starting",
            ServerHealth::Ready => "ready",
            ServerHealth::Backoff => "backoff",
            ServerHealth::Unhealthy => "unhealthy",
        }
    }
}

/// Messages name setting keys only, never values.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PoolError {
    #[error("unavailable")]
    Unavailable,
    #[error("{0}")]
    Unhealthy(String),
    #[error("{0}")]
    Config(String),
    #[error("timed out")]
    Timeout,
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub start_timeout: Duration,
    pub idle_shutdown: Duration,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
    pub unhealthy_after: u32,
}

impl PoolConfig {
    pub fn from_plugins(cfg: &PluginsConfig) -> Self {
        Self {
            start_timeout: Duration::from_secs(cfg.mcp_start_timeout_secs),
            idle_shutdown: Duration::from_secs(cfg.mcp_idle_shutdown_secs),
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            unhealthy_after: 3,
        }
    }
}

const REAP_INTERVAL: Duration = Duration::from_secs(30);
const CLOSED: &str = "server exited or closed the connection";
const START_ABORTED: &str = "start task failed";

struct Instance {
    conn: Arc<dyn McpConnection>,
    /// Async lock so concurrent `tools()` callers share one `tools/list`.
    tools: tokio::sync::Mutex<Option<Vec<RemoteTool>>>,
}

impl Instance {
    async fn tools(&self) -> Result<Vec<RemoteTool>, ProxyError> {
        let mut cache = self.tools.lock().await;
        let changed = self.conn.take_tools_changed();
        if let (Some(tools), false) = (cache.as_ref(), changed) {
            return Ok(tools.clone());
        }
        *cache = None;
        let tools = self.conn.list_tools().await?;
        *cache = Some(tools.clone());
        Ok(tools)
    }
}

type StartResult = Option<Result<Arc<Instance>, PoolError>>;

enum Phase {
    Stopped,
    Starting(watch::Receiver<StartResult>),
    Ready(Arc<Instance>),
    Backoff(Instant),
    Unhealthy,
}

struct Slot {
    fingerprint: Option<[u8; 32]>,
    phase: Phase,
    failures: u32,
    /// Redacted by the transport or a key-only placeholder error.
    last_error: String,
    last_used: Instant,
    /// Bumped on every start and stop so a superseded start discards its connection.
    generation: u64,
}

impl Slot {
    fn new() -> Self {
        Self {
            fingerprint: None,
            phase: Phase::Stopped,
            failures: 0,
            last_error: String::new(),
            last_used: Instant::now(),
            generation: 0,
        }
    }

    /// Back to a fresh `Stopped` slot; returns the running instance to close.
    fn reset(&mut self) -> Option<Arc<Instance>> {
        self.generation += 1;
        self.failures = 0;
        self.last_error.clear();
        self.fingerprint = None;
        match std::mem::replace(&mut self.phase, Phase::Stopped) {
            Phase::Ready(instance) => Some(instance),
            _ => None,
        }
    }

    fn record_failure(&mut self, cfg: &PoolConfig, error: String) {
        self.failures += 1;
        self.last_error = error;
        self.phase = if self.failures >= cfg.unhealthy_after {
            Phase::Unhealthy
        } else {
            let factor = 2u32.saturating_pow(self.failures - 1);
            let delay = cfg
                .backoff_initial
                .saturating_mul(factor)
                .min(cfg.backoff_max);
            Phase::Backoff(Instant::now() + delay)
        };
    }
}

type SlotRef = Arc<Mutex<Slot>>;
type Prepared = Result<(ResolvedTransport, Arc<dyn McpTransport>), String>;

enum Step {
    Done(Result<Arc<Instance>, PoolError>),
    Wait(watch::Receiver<StartResult>),
}

/// State per key lives behind small std mutexes that are never held across `.await`;
/// starts run in spawned tasks so a cancelled caller cannot abort or wedge them.
pub struct McpServerPool {
    transports: Transports,
    cfg: PoolConfig,
    slots: Mutex<HashMap<ServerKey, SlotRef>>,
}

impl McpServerPool {
    pub fn new(transports: Transports, cfg: PoolConfig) -> Self {
        Self {
            transports,
            cfg,
            slots: Mutex::new(HashMap::new()),
        }
    }

    pub async fn tools(&self, spec: &PoolServerSpec) -> Result<Vec<RemoteTool>, PoolError> {
        let instance = self.instance(spec).await?;
        let result = instance.tools().await;
        self.touch(&spec.key);
        match result {
            Ok(tools) => Ok(tools),
            Err(err) => Err(self.request_error(&spec.key, &instance, err)),
        }
    }

    /// A tool-level JSON-RPC error on a live connection becomes an error result.
    pub async fn call(
        &self,
        spec: &PoolServerSpec,
        tool: &str,
        args: Value,
    ) -> Result<ToolResult, PoolError> {
        let instance = self.instance(spec).await?;
        let result = instance.conn.call_tool(tool, args).await;
        self.touch(&spec.key);
        match result {
            Ok(result) => {
                if instance.conn.is_closed() {
                    self.fail_instance(&spec.key, &instance);
                }
                Ok(result)
            }
            Err(ProxyError::Protocol(message)) if !instance.conn.is_closed() => Ok(ToolResult {
                content: vec![ToolContent::Text(message)],
                is_error: true,
            }),
            Err(err) => Err(self.request_error(&spec.key, &instance, err)),
        }
    }

    /// Stops the instance, clears its failures, and starts it fresh.
    pub async fn test(&self, spec: &PoolServerSpec) -> Result<Vec<RemoteTool>, PoolError> {
        let stale = self.slot(&spec.key).lock().unwrap().reset();
        if let Some(instance) = stale {
            instance.conn.close().await;
        }
        self.tools(spec).await
    }

    /// Redacted reason for the last failure; cleared by a successful start or a stop.
    pub fn last_error(&self, key: &ServerKey) -> Option<String> {
        let slot = self.slots.lock().unwrap().get(key).cloned()?;
        let slot = slot.lock().unwrap();
        (!slot.last_error.is_empty()).then(|| slot.last_error.clone())
    }

    pub fn health(&self, key: &ServerKey) -> ServerHealth {
        let Some(slot) = self.slots.lock().unwrap().get(key).cloned() else {
            return ServerHealth::Stopped;
        };
        let slot = slot.lock().unwrap();
        match slot.phase {
            Phase::Stopped => ServerHealth::Stopped,
            Phase::Starting(_) => ServerHealth::Starting,
            Phase::Ready(_) => ServerHealth::Ready,
            Phase::Backoff(_) => ServerHealth::Backoff,
            Phase::Unhealthy => ServerHealth::Unhealthy,
        }
    }

    pub async fn stop_plugin(&self, plugin_id: Uuid) {
        let stale: Vec<_> = self
            .all_slots()
            .into_iter()
            .filter(|(key, _)| key.plugin_id == plugin_id)
            .filter_map(|(_, slot)| slot.lock().unwrap().reset())
            .collect();
        close_all(stale).await;
    }

    pub async fn reap_idle(&self) {
        let now = Instant::now();
        let stale: Vec<_> = self
            .all_slots()
            .into_iter()
            .filter_map(|(_, slot)| {
                let mut slot = slot.lock().unwrap();
                let idle = now.saturating_duration_since(slot.last_used) >= self.cfg.idle_shutdown;
                (matches!(slot.phase, Phase::Ready(_)) && idle)
                    .then(|| slot.reset())
                    .flatten()
            })
            .collect();
        close_all(stale).await;
    }

    pub fn spawn_reaper(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let pool = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut ticks = tokio::time::interval_at(Instant::now() + REAP_INTERVAL, REAP_INTERVAL);
            loop {
                ticks.tick().await;
                let Some(pool) = pool.upgrade() else {
                    return;
                };
                pool.reap_idle().await;
            }
        })
    }

    fn slot(&self, key: &ServerKey) -> SlotRef {
        self.slots
            .lock()
            .unwrap()
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Mutex::new(Slot::new())))
            .clone()
    }

    fn all_slots(&self) -> Vec<(ServerKey, SlotRef)> {
        self.slots
            .lock()
            .unwrap()
            .iter()
            .map(|(key, slot)| (key.clone(), slot.clone()))
            .collect()
    }

    fn touch(&self, key: &ServerKey) {
        self.slot(key).lock().unwrap().last_used = Instant::now();
    }

    fn prepare(&self, spec: &PoolServerSpec) -> ([u8; 32], Prepared) {
        let env = |name: &str| {
            server_env_allowed(name)
                .then(|| std::env::var(name).ok())
                .flatten()
        };
        let ctx = ResolveCtx {
            plugin_root: &spec.plugin_root,
            settings: &spec.settings,
            env: &env,
        };
        match resolve(&spec.entry.transport, &ctx) {
            Ok(resolved) => {
                let fingerprint = fingerprint(&resolved, &spec.plugin_root);
                let prepared = match self.transports.get(resolved.kind()) {
                    Some(transport) => Ok((resolved, transport)),
                    None => Err(format!("unsupported transport \"{}\"", resolved.kind())),
                };
                (fingerprint, prepared)
            }
            Err(message) => (error_fingerprint(&message), Err(message)),
        }
    }

    async fn instance(&self, spec: &PoolServerSpec) -> Result<Arc<Instance>, PoolError> {
        let (fingerprint, prepared) = self.prepare(spec);
        let slot_ref = self.slot(&spec.key);
        let (step, stale) = {
            let mut slot = slot_ref.lock().unwrap();
            slot.last_used = Instant::now();
            let mut stale = None;
            if slot.fingerprint != Some(fingerprint) {
                stale = slot.reset();
                slot.fingerprint = Some(fingerprint);
            }
            let step = match &slot.phase {
                Phase::Ready(instance) if !instance.conn.is_closed() => {
                    Step::Done(Ok(instance.clone()))
                }
                Phase::Ready(instance) => {
                    stale = Some(instance.clone());
                    slot.record_failure(&self.cfg, CLOSED.into());
                    Step::Done(Err(PoolError::Unavailable))
                }
                Phase::Starting(rx) => Step::Wait(rx.clone()),
                Phase::Backoff(until) if Instant::now() < *until => {
                    Step::Done(Err(PoolError::Unavailable))
                }
                Phase::Unhealthy => Step::Done(Err(PoolError::Unhealthy(slot.last_error.clone()))),
                Phase::Stopped | Phase::Backoff(_) => match prepared {
                    Err(message) => {
                        slot.phase = Phase::Unhealthy;
                        slot.last_error = message.clone();
                        Step::Done(Err(PoolError::Config(message)))
                    }
                    Ok((resolved, transport)) => Step::Wait(self.start(
                        &mut slot,
                        slot_ref.clone(),
                        resolved,
                        transport,
                        spec.plugin_root.clone(),
                    )),
                },
            };
            (step, stale)
        };
        if let Some(instance) = stale {
            close_detached(instance);
        }
        match step {
            Step::Done(result) => result,
            Step::Wait(mut rx) => match rx.wait_for(Option::is_some).await {
                Ok(result) => result.clone().unwrap_or(Err(PoolError::Unavailable)),
                Err(_) => Err(PoolError::Unavailable),
            },
        }
    }

    fn start(
        &self,
        slot: &mut Slot,
        slot_ref: SlotRef,
        resolved: ResolvedTransport,
        transport: Arc<dyn McpTransport>,
        cwd: PathBuf,
    ) -> watch::Receiver<StartResult> {
        slot.generation += 1;
        let generation = slot.generation;
        let (tx, rx) = watch::channel(None);
        slot.phase = Phase::Starting(rx.clone());
        let cfg = self.cfg.clone();
        tokio::spawn(async move {
            let mut guard = StartGuard {
                slot_ref,
                generation,
                cfg,
                tx: Some(tx),
            };
            let outcome = match tokio::time::timeout(
                guard.cfg.start_timeout,
                transport.connect(&resolved, &cwd),
            )
            .await
            {
                Ok(Ok(conn)) => Ok(conn),
                Ok(Err(err)) => Err((PoolError::Unavailable, err.to_string())),
                Err(_) => Err((
                    PoolError::Timeout,
                    format!(
                        "start timed out after {}s",
                        guard.cfg.start_timeout.as_secs()
                    ),
                )),
            };
            if let Some(orphan) = guard.finish(outcome) {
                orphan.close().await;
            }
        });
        rx
    }

    /// Counts a closed connection as a failure; other errors leave the instance running.
    fn request_error(
        &self,
        key: &ServerKey,
        instance: &Arc<Instance>,
        err: ProxyError,
    ) -> PoolError {
        if matches!(err, ProxyError::Closed) || instance.conn.is_closed() {
            self.fail_instance(key, instance);
            return PoolError::Unavailable;
        }
        match err {
            ProxyError::Timeout => PoolError::Timeout,
            _ => PoolError::Unavailable,
        }
    }

    fn fail_instance(&self, key: &ServerKey, instance: &Arc<Instance>) {
        let slot_ref = self.slot(key);
        let failed = {
            let mut slot = slot_ref.lock().unwrap();
            let current = matches!(&slot.phase, Phase::Ready(cur) if Arc::ptr_eq(cur, instance));
            if current {
                slot.record_failure(&self.cfg, CLOSED.into());
            }
            current
        };
        if failed {
            close_detached(instance.clone());
        }
    }
}

/// Owns a start's result channel: a start task that panics or is aborted before
/// reporting still counts as a failed start instead of wedging the slot in `Starting`.
struct StartGuard {
    slot_ref: SlotRef,
    generation: u64,
    cfg: PoolConfig,
    tx: Option<watch::Sender<StartResult>>,
}

impl StartGuard {
    /// Installs the outcome; returns a connection that a stop or restart superseded.
    fn finish(
        &mut self,
        outcome: Result<Box<dyn McpConnection>, (PoolError, String)>,
    ) -> Option<Box<dyn McpConnection>> {
        let (result, orphan) = {
            let mut slot = self.slot_ref.lock().unwrap();
            if slot.generation != self.generation {
                (Err(PoolError::Unavailable), outcome.ok())
            } else {
                match outcome {
                    Ok(conn) => {
                        let instance = Arc::new(Instance {
                            conn: Arc::from(conn),
                            tools: tokio::sync::Mutex::new(None),
                        });
                        slot.phase = Phase::Ready(instance.clone());
                        slot.failures = 0;
                        slot.last_error.clear();
                        (Ok(instance), None)
                    }
                    Err((err, message)) => {
                        slot.record_failure(&self.cfg, message);
                        (Err(err), None)
                    }
                }
            }
        };
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Some(result));
        }
        orphan
    }
}

impl Drop for StartGuard {
    fn drop(&mut self) {
        let Some(tx) = self.tx.take() else {
            return;
        };
        {
            let mut slot = self
                .slot_ref
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if slot.generation == self.generation && matches!(slot.phase, Phase::Starting(_)) {
                slot.record_failure(&self.cfg, START_ABORTED.into());
            }
        }
        let _ = tx.send(Some(Err(PoolError::Unavailable)));
    }
}

/// Closes on a spawned task so a cancelled caller cannot interrupt shutdown.
fn close_detached(instance: Arc<Instance>) {
    tokio::spawn(async move { instance.conn.close().await });
}

async fn close_all(instances: Vec<Arc<Instance>>) {
    futures_util::future::join_all(instances.iter().map(|i| i.conn.close())).await;
}

fn hash_str(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn hash_map(hasher: &mut Sha256, map: &BTreeMap<String, String>) {
    hasher.update((map.len() as u64).to_le_bytes());
    for (key, value) in map {
        hash_str(hasher, key);
        hash_str(hasher, value);
    }
}

/// Never log the hashed fields: they hold resolved secrets.
fn fingerprint(resolved: &ResolvedTransport, cwd: &Path) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hash_str(&mut hasher, resolved.kind());
    hash_str(&mut hasher, &cwd.to_string_lossy());
    match resolved {
        ResolvedTransport::Stdio {
            command, args, env, ..
        } => {
            hash_str(&mut hasher, command);
            hasher.update((args.len() as u64).to_le_bytes());
            for arg in args {
                hash_str(&mut hasher, arg);
            }
            hash_map(&mut hasher, env);
        }
        ResolvedTransport::Http { url, headers, .. } => {
            hash_str(&mut hasher, url);
            hash_map(&mut hasher, headers);
        }
    }
    hasher.finalize().into()
}

fn error_fingerprint(message: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hash_str(&mut hasher, "config-error");
    hash_str(&mut hasher, message);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::capability::McpServerTransport;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

    #[derive(Default)]
    struct FakeConn {
        closed: AtomicBool,
        changed: AtomicBool,
        lists: AtomicUsize,
    }

    #[derive(Default)]
    struct Fake {
        connects: AtomicUsize,
        fail_starts: AtomicBool,
        panic_on_connect: AtomicBool,
        connect_delay_ms: AtomicU64,
        close_on_call: AtomicBool,
        conns: Mutex<Vec<Arc<FakeConn>>>,
    }

    impl Fake {
        fn connects(&self) -> usize {
            self.connects.load(Ordering::SeqCst)
        }

        fn conn(&self, index: usize) -> Arc<FakeConn> {
            self.conns.lock().unwrap()[index].clone()
        }
    }

    struct FakeTransport(Arc<Fake>);

    struct FakeHandle {
        fake: Arc<Fake>,
        conn: Arc<FakeConn>,
    }

    #[async_trait]
    impl McpTransport for FakeTransport {
        fn kind(&self) -> &'static str {
            "stdio"
        }

        async fn connect(
            &self,
            _spec: &ResolvedTransport,
            _cwd: &Path,
        ) -> Result<Box<dyn McpConnection>, ProxyError> {
            self.0.connects.fetch_add(1, Ordering::SeqCst);
            let delay = self.0.connect_delay_ms.load(Ordering::SeqCst);
            if delay > 0 {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            if self.0.panic_on_connect.load(Ordering::SeqCst) {
                panic!("fake connect panicked");
            }
            if self.0.fail_starts.load(Ordering::SeqCst) {
                return Err(ProxyError::Start("boom".into()));
            }
            let conn = Arc::new(FakeConn::default());
            self.0.conns.lock().unwrap().push(conn.clone());
            Ok(Box::new(FakeHandle {
                fake: self.0.clone(),
                conn,
            }))
        }
    }

    #[async_trait]
    impl McpConnection for FakeHandle {
        async fn list_tools(&self) -> Result<Vec<RemoteTool>, ProxyError> {
            self.conn.lists.fetch_add(1, Ordering::SeqCst);
            Ok(vec![RemoteTool {
                name: "echo".into(),
                description: String::new(),
                input_schema: serde_json::json!({ "type": "object" }),
                read_only: true,
            }])
        }

        async fn call_tool(&self, name: &str, _args: Value) -> Result<ToolResult, ProxyError> {
            if name == "sleep" {
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
            if self.fake.close_on_call.load(Ordering::SeqCst) {
                self.conn.closed.store(true, Ordering::SeqCst);
                return Err(ProxyError::Closed);
            }
            Ok(ToolResult {
                content: vec![ToolContent::Text(name.to_string())],
                is_error: false,
            })
        }

        fn take_tools_changed(&self) -> bool {
            self.conn.changed.swap(false, Ordering::SeqCst)
        }

        fn is_closed(&self) -> bool {
            self.conn.closed.load(Ordering::SeqCst)
        }

        async fn close(&self) {
            self.conn.closed.store(true, Ordering::SeqCst);
        }
    }

    fn cfg() -> PoolConfig {
        PoolConfig {
            start_timeout: Duration::from_secs(20),
            idle_shutdown: Duration::from_secs(600),
            backoff_initial: Duration::from_secs(1),
            backoff_max: Duration::from_secs(60),
            unhealthy_after: 3,
        }
    }

    fn pool() -> (Arc<McpServerPool>, Arc<Fake>) {
        let fake = Arc::new(Fake::default());
        let mut transports = Transports::default();
        transports.register(Arc::new(FakeTransport(fake.clone())));
        (Arc::new(McpServerPool::new(transports, cfg())), fake)
    }

    fn spec_for(plugin_id: Uuid, server: &str, token: Option<&str>) -> PoolServerSpec {
        PoolServerSpec {
            key: ServerKey {
                plugin_id,
                server: server.into(),
            },
            plugin_name: "demo".into(),
            plugin_root: PathBuf::from("/plugins/demo"),
            entry: McpServerEntry {
                name: server.into(),
                transport: McpServerTransport::Stdio {
                    command: "fake".into(),
                    args: Vec::new(),
                    env: [("TOKEN".to_string(), "${API_TOKEN}".to_string())].into(),
                },
                error: None,
            },
            settings: token
                .map(|t| [("API_TOKEN".to_string(), t.to_string())].into())
                .unwrap_or_default(),
        }
    }

    fn spec() -> PoolServerSpec {
        spec_for(Uuid::nil(), "main", Some("tok"))
    }

    async fn settle() {
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[test]
    fn config_from_plugins_defaults() {
        let cfg = PoolConfig::from_plugins(&PluginsConfig::default());
        assert_eq!(cfg.start_timeout, Duration::from_secs(20));
        assert_eq!(cfg.idle_shutdown, Duration::from_secs(600));
        assert_eq!(cfg.backoff_initial, Duration::from_secs(1));
        assert_eq!(cfg.backoff_max, Duration::from_secs(60));
        assert_eq!(cfg.unhealthy_after, 3);
        assert_eq!(ServerHealth::Backoff.as_str(), "backoff");
        assert_eq!(PoolError::Unavailable.to_string(), "unavailable");
    }

    #[tokio::test(start_paused = true)]
    async fn lazy_start_once_for_concurrent_callers() {
        let (pool, fake) = pool();
        fake.connect_delay_ms.store(100, Ordering::SeqCst);
        assert_eq!(pool.health(&spec().key), ServerHealth::Stopped);
        let spec = spec();
        let calls = (0..5).map(|_| pool.tools(&spec));
        let results = futures_util::future::join_all(calls).await;
        for result in results {
            assert_eq!(result.unwrap().len(), 1);
        }
        assert_eq!(fake.connects(), 1);
        assert_eq!(pool.health(&spec.key), ServerHealth::Ready);
    }

    #[tokio::test(start_paused = true)]
    async fn tools_cached_until_changed() {
        let (pool, fake) = pool();
        pool.tools(&spec()).await.unwrap();
        pool.tools(&spec()).await.unwrap();
        let conn = fake.conn(0);
        assert_eq!(conn.lists.load(Ordering::SeqCst), 1);
        conn.changed.store(true, Ordering::SeqCst);
        pool.tools(&spec()).await.unwrap();
        assert_eq!(conn.lists.load(Ordering::SeqCst), 2);
        assert_eq!(fake.connects(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn crash_then_backoff_restart() {
        let (pool, fake) = pool();
        pool.tools(&spec()).await.unwrap();
        fake.close_on_call.store(true, Ordering::SeqCst);
        assert_eq!(
            pool.call(&spec(), "echo", Value::Null).await.err(),
            Some(PoolError::Unavailable)
        );
        assert_eq!(pool.health(&spec().key), ServerHealth::Backoff);
        fake.close_on_call.store(false, Ordering::SeqCst);

        tokio::time::advance(Duration::from_millis(500)).await;
        assert_eq!(
            pool.call(&spec(), "echo", Value::Null).await.err(),
            Some(PoolError::Unavailable)
        );
        assert_eq!(fake.connects(), 1);

        tokio::time::advance(Duration::from_millis(600)).await;
        let result = pool.call(&spec(), "echo", Value::Null).await.unwrap();
        assert_eq!(result.content, vec![ToolContent::Text("echo".into())]);
        assert_eq!(fake.connects(), 2);
        assert_eq!(pool.health(&spec().key), ServerHealth::Ready);
    }

    #[tokio::test(start_paused = true)]
    async fn three_failures_unhealthy_hides_tools() {
        let (pool, fake) = pool();
        fake.fail_starts.store(true, Ordering::SeqCst);
        assert_eq!(pool.tools(&spec()).await, Err(PoolError::Unavailable));
        assert_eq!(pool.health(&spec().key), ServerHealth::Backoff);
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(pool.tools(&spec()).await, Err(PoolError::Unavailable));
        // Second backoff doubles to 2 s.
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(pool.tools(&spec()).await, Err(PoolError::Unavailable));
        assert_eq!(fake.connects(), 2);
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(pool.tools(&spec()).await, Err(PoolError::Unavailable));
        assert_eq!(fake.connects(), 3);
        assert_eq!(pool.health(&spec().key), ServerHealth::Unhealthy);

        tokio::time::advance(Duration::from_secs(120)).await;
        assert!(matches!(
            pool.tools(&spec()).await,
            Err(PoolError::Unhealthy(_))
        ));
        assert_eq!(fake.connects(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn test_clears_unhealthy() {
        let (pool, fake) = pool();
        fake.fail_starts.store(true, Ordering::SeqCst);
        for _ in 0..3 {
            let _ = pool.tools(&spec()).await;
            tokio::time::advance(Duration::from_secs(10)).await;
        }
        assert_eq!(pool.health(&spec().key), ServerHealth::Unhealthy);
        fake.fail_starts.store(false, Ordering::SeqCst);
        assert_eq!(pool.test(&spec()).await.unwrap().len(), 1);
        assert_eq!(pool.health(&spec().key), ServerHealth::Ready);
    }

    #[tokio::test(start_paused = true)]
    async fn config_error_is_unhealthy_immediately() {
        let (pool, fake) = pool();
        let spec = spec_for(Uuid::nil(), "main", None);
        assert_eq!(
            pool.tools(&spec).await,
            Err(PoolError::Config("missing setting \"API_TOKEN\"".into()))
        );
        assert_eq!(pool.health(&spec.key), ServerHealth::Unhealthy);
        assert_eq!(
            pool.call(&spec, "echo", Value::Null).await.err(),
            Some(PoolError::Unhealthy("missing setting \"API_TOKEN\"".into()))
        );
        assert_eq!(fake.connects(), 0);

        assert!(pool.tools(&self::spec()).await.is_ok());
        assert_eq!(fake.connects(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn unregistered_transport_is_config_error() {
        let (pool, fake) = pool();
        let mut spec = spec();
        spec.entry.transport = McpServerTransport::Http {
            url: "http://127.0.0.1:1/mcp".into(),
            headers: BTreeMap::new(),
        };
        assert_eq!(
            pool.tools(&spec).await,
            Err(PoolError::Config("unsupported transport \"http\"".into()))
        );
        assert_eq!(pool.health(&spec.key), ServerHealth::Unhealthy);
        assert_eq!(fake.connects(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn fingerprint_change_restarts() {
        let (pool, fake) = pool();
        pool.tools(&spec()).await.unwrap();
        let changed = spec_for(Uuid::nil(), "main", Some("other"));
        pool.tools(&changed).await.unwrap();
        settle().await;
        assert!(fake.conn(0).closed.load(Ordering::SeqCst));
        assert!(!fake.conn(1).closed.load(Ordering::SeqCst));
        assert_eq!(fake.connects(), 2);
        pool.tools(&changed).await.unwrap();
        assert_eq!(fake.connects(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn idle_reaped() {
        let (pool, fake) = pool();
        pool.tools(&spec()).await.unwrap();
        tokio::time::advance(Duration::from_secs(599)).await;
        pool.reap_idle().await;
        assert_eq!(pool.health(&spec().key), ServerHealth::Ready);
        tokio::time::advance(Duration::from_secs(1)).await;
        pool.reap_idle().await;
        assert!(fake.conn(0).closed.load(Ordering::SeqCst));
        assert_eq!(pool.health(&spec().key), ServerHealth::Stopped);
    }

    #[tokio::test(start_paused = true)]
    async fn reaper_task_reaps() {
        let (pool, fake) = pool();
        let reaper = pool.spawn_reaper();
        pool.tools(&spec()).await.unwrap();
        tokio::time::sleep(Duration::from_secs(660)).await;
        assert!(fake.conn(0).closed.load(Ordering::SeqCst));
        assert_eq!(pool.health(&spec().key), ServerHealth::Stopped);
        reaper.abort();
    }

    #[tokio::test(start_paused = true)]
    async fn stop_plugin_closes_all_its_servers() {
        let (pool, fake) = pool();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let specs = [
            spec_for(a, "one", Some("t")),
            spec_for(a, "two", Some("t")),
            spec_for(b, "one", Some("t")),
        ];
        for spec in &specs {
            pool.tools(spec).await.unwrap();
        }
        pool.stop_plugin(a).await;
        assert!(fake.conn(0).closed.load(Ordering::SeqCst));
        assert!(fake.conn(1).closed.load(Ordering::SeqCst));
        assert!(!fake.conn(2).closed.load(Ordering::SeqCst));
        assert_eq!(pool.health(&specs[0].key), ServerHealth::Stopped);
        assert_eq!(pool.health(&specs[1].key), ServerHealth::Stopped);
        assert_eq!(pool.health(&specs[2].key), ServerHealth::Ready);
    }

    #[tokio::test(start_paused = true)]
    async fn start_continues_after_caller_timeout() {
        let (pool, fake) = pool();
        fake.connect_delay_ms.store(3000, Ordering::SeqCst);
        let spec = spec();
        let gave_up = tokio::time::timeout(Duration::from_secs(1), pool.tools(&spec)).await;
        assert!(gave_up.is_err());
        assert_eq!(pool.health(&spec.key), ServerHealth::Starting);
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(pool.health(&spec.key), ServerHealth::Ready);
        pool.tools(&spec).await.unwrap();
        assert_eq!(fake.connects(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn start_timeout_counts_as_failure() {
        let (pool, fake) = pool();
        fake.connect_delay_ms.store(30_000, Ordering::SeqCst);
        assert_eq!(pool.tools(&spec()).await, Err(PoolError::Timeout));
        assert_eq!(pool.health(&spec().key), ServerHealth::Backoff);
    }

    #[tokio::test(start_paused = true)]
    async fn caller_timeout_during_call_keeps_state() {
        let (pool, fake) = pool();
        let spec = spec();
        pool.tools(&spec).await.unwrap();
        let slow = pool.call(&spec, "sleep", Value::Null);
        assert!(tokio::time::timeout(Duration::from_secs(1), slow)
            .await
            .is_err());
        assert_eq!(pool.health(&spec.key), ServerHealth::Ready);
        assert!(pool.call(&spec, "echo", Value::Null).await.is_ok());
        assert_eq!(fake.connects(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn panicked_start_counts_as_failure() {
        let (pool, fake) = pool();
        fake.panic_on_connect.store(true, Ordering::SeqCst);
        assert_eq!(pool.tools(&spec()).await, Err(PoolError::Unavailable));
        assert_eq!(pool.health(&spec().key), ServerHealth::Backoff);
        assert_eq!(
            pool.last_error(&spec().key).as_deref(),
            Some("start task failed")
        );
        fake.panic_on_connect.store(false, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(pool.tools(&spec()).await.unwrap().len(), 1);
        assert_eq!(pool.health(&spec().key), ServerHealth::Ready);
        assert_eq!(fake.connects(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn last_error_records_failures_without_secrets() {
        let (pool, fake) = pool();
        let key = spec().key;
        assert_eq!(pool.last_error(&key), None);

        fake.fail_starts.store(true, Ordering::SeqCst);
        let _ = pool.tools(&spec()).await;
        let message = pool.last_error(&key).unwrap();
        assert_eq!(message, "MCP server failed to start: boom");
        assert!(!message.contains("tok"));

        fake.fail_starts.store(false, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(1)).await;
        pool.tools(&spec()).await.unwrap();
        assert_eq!(pool.last_error(&key), None);

        fake.close_on_call.store(true, Ordering::SeqCst);
        let _ = pool.call(&spec(), "echo", Value::Null).await;
        assert_eq!(
            pool.last_error(&key).as_deref(),
            Some("server exited or closed the connection")
        );
        fake.close_on_call.store(false, Ordering::SeqCst);

        fake.connect_delay_ms.store(30_000, Ordering::SeqCst);
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(pool.tools(&spec()).await, Err(PoolError::Timeout));
        assert_eq!(
            pool.last_error(&key).as_deref(),
            Some("start timed out after 20s")
        );

        let missing = spec_for(Uuid::nil(), "main", None);
        let _ = pool.tools(&missing).await;
        assert_eq!(
            pool.last_error(&key).as_deref(),
            Some("missing setting \"API_TOKEN\"")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stop_plugin_during_start_closes_orphan() {
        let (pool, fake) = pool();
        fake.connect_delay_ms.store(3000, Ordering::SeqCst);
        let waiter = {
            let pool = pool.clone();
            tokio::spawn(async move { pool.tools(&spec()).await })
        };
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(pool.health(&spec().key), ServerHealth::Starting);
        pool.stop_plugin(Uuid::nil()).await;
        assert_eq!(pool.health(&spec().key), ServerHealth::Stopped);

        assert_eq!(waiter.await.unwrap(), Err(PoolError::Unavailable));
        assert!(fake.conn(0).closed.load(Ordering::SeqCst));
        assert_eq!(pool.health(&spec().key), ServerHealth::Stopped);
        assert_eq!(pool.last_error(&spec().key), None);
        assert_eq!(fake.connects(), 1);
    }
}
