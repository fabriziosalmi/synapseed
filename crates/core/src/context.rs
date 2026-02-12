use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::event::SynapseEvent;
use crate::liquid::ProjectDna;
use crate::state::ProjectState;

/// Thread-safe shared state + async Event Bus for all plugins.
///
/// The context is both a data store (project state, metrics, config)
/// and a nervous system (broadcast channel for domain events).
///
/// Cloning is cheap (Arc internally).
#[derive(Clone)]
pub struct SynapseContext {
    inner: Arc<RwLock<ContextInner>>,
    /// Async broadcast sender — plugins subscribe via `subscribe()`
    event_tx: broadcast::Sender<SynapseEvent>,
    /// Type-erased extensions — plugins register shared objects for cross-crate access.
    extensions: Arc<RwLock<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>>,
    /// Cancellation token for coordinated async shutdown.
    shutdown_token: CancellationToken,
    /// Companion flag for std::thread loops that cannot await.
    shutdown_flag: Arc<AtomicBool>,
}

struct ContextInner {
    project_root: PathBuf,
    project_state: ProjectState,
    dna: ProjectDna,
    metrics: ContextMetrics,
}

/// Runtime metrics tracked across the session.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct ContextMetrics {
    pub files_indexed: usize,
    pub symbols_found: usize,
    pub dlp_scans: usize,
    pub dlp_blocks: usize,
    pub commands_allowed: usize,
    pub commands_denied: usize,
    pub errors_prevented: usize,
    pub events_broadcast: usize,
    pub tools_invoked: usize,
}

impl SynapseContext {
    /// Create a new context with an event bus (capacity = 4096 events).
    ///
    /// A large capacity prevents `Lagged` errors when plugins produce
    /// events faster than consumers drain them (e.g., bulk file indexing).
    pub fn new(project_root: PathBuf, state: ProjectState, dna: ProjectDna) -> Self {
        let (event_tx, _) = broadcast::channel(4096);

        Self {
            inner: Arc::new(RwLock::new(ContextInner {
                project_root,
                project_state: state,
                dna,
                metrics: ContextMetrics::default(),
            })),
            event_tx,
            extensions: Arc::new(RwLock::new(HashMap::new())),
            shutdown_token: CancellationToken::new(),
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    // ── Data Accessors ──────────────────────────────────────────

    pub fn project_root(&self) -> PathBuf {
        self.inner.read().unwrap().project_root.clone()
    }

    pub fn project_state(&self) -> ProjectState {
        self.inner.read().unwrap().project_state.clone()
    }

    pub fn dna(&self) -> ProjectDna {
        self.inner.read().unwrap().dna.clone()
    }

    pub fn metrics(&self) -> ContextMetrics {
        self.inner.read().unwrap().metrics.clone()
    }

    pub fn update_metrics<F: FnOnce(&mut ContextMetrics)>(&self, f: F) {
        let mut inner = self.inner.write().unwrap();
        f(&mut inner.metrics);
    }

    pub fn set_project_state(&self, state: ProjectState) {
        self.inner.write().unwrap().project_state = state;
    }

    // ── Event Bus ───────────────────────────────────────────────

    /// Broadcast an event to all subscribers.
    /// Returns the number of receivers that got the event.
    pub fn broadcast(&self, event: SynapseEvent) -> usize {
        debug!(event_type = ?std::mem::discriminant(&event), "Broadcasting event");
        self.update_metrics(|m| m.events_broadcast += 1);
        self.event_tx.send(event).unwrap_or(0)
    }

    /// Subscribe to the event bus. Returns a receiver for async consumption.
    pub fn subscribe(&self) -> broadcast::Receiver<SynapseEvent> {
        self.event_tx.subscribe()
    }

    /// Get a clone of the sender (for spawning background tasks that emit events).
    pub fn event_sender(&self) -> broadcast::Sender<SynapseEvent> {
        self.event_tx.clone()
    }

    // ── Extensions ──────────────────────────────────────────────

    /// Register a shared object by type. Plugins set these during on_init.
    pub fn set_extension<T: Send + Sync + 'static>(&self, ext: Arc<T>) {
        let mut map = self.extensions.write().unwrap();
        map.insert(TypeId::of::<T>(), ext);
    }

    /// Retrieve a shared object by type. MCP tools and other plugins read these.
    pub fn get_extension<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let map = self.extensions.read().unwrap();
        map.get(&TypeId::of::<T>())
            .and_then(|ext| ext.clone().downcast::<T>().ok())
    }

    // ── Shutdown Coordination ────────────────────────────────────

    /// Returns a clone of the cancellation token.
    /// Background async tasks should use `token.cancelled().await` in a
    /// `tokio::select!` branch to exit their loops.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Returns a clone of the shutdown flag for std::thread loops.
    /// Check with `flag.load(Ordering::Relaxed)` in loop conditions.
    pub fn shutdown_flag(&self) -> Arc<AtomicBool> {
        self.shutdown_flag.clone()
    }

    /// Trigger shutdown: sets the flag and cancels the token.
    /// Called once from the signal handler in main.rs.
    pub fn request_shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::Relaxed);
        self.shutdown_token.cancel();
    }

    /// Returns true if shutdown has been requested.
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_token.is_cancelled()
    }
}
