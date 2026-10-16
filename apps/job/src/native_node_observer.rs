//! Read-only, opt-in hookup to an existing official Nautilus LiveNode.
//! The owner drives the native lifecycle and calls heartbeat on its event thread.
//! No node, execution client, trading command or financial calculation is created here.
use crate::{account_observation_cli as retained, account_observer::NativeAccountObserver};
use anyhow::{anyhow, bail, Result};
use contracts::{account_observation::*, forward::ForwardEnvironmentV1, DbCounter, Id, SchemaV1};
use nautilus_common::{
    enums::Environment,
    msgbus::{self, TypedHandler},
};
use nautilus_execution::engine::ExecutionEngine;
use nautilus_live::node::{LiveNode, LiveNodeHandle, NodeState};
use nautilus_model::{events::PortfolioSnapshot, identifiers::ClientId};
use std::{
    cell::RefCell,
    fs::File,
    path::Path,
    rc::Rc,
    sync::mpsc::{self, SyncSender},
    thread::JoinHandle,
};

/// Subscribe before running the node. Keep this on the same native event thread.
/// Call heartbeat at least every 30 seconds, including while the account is flat.
/// Call finish only AFTER the native run/stop completes, to retain its final snapshot.
/// Dropping without finish detaches but does not claim durable or clean shutdown.
pub struct NativeNodeObserver {
    observer: NativeAccountObserver,
    sender: Option<SyncSender<AccountObservationSubmitV1>>,
    handler: Option<TypedHandler<PortfolioSnapshot>>,
    topic: String,
    handle: LiveNodeHandle,
    engine: Rc<RefCell<ExecutionEngine>>,
    client_id: ClientId,
    native_client_id: Option<String>,
    native_account_id: String,
    writer: Option<JoinHandle<Result<File>>>,
}

fn connection(
    handle: &LiveNodeHandle,
    engine: &RefCell<ExecutionEngine>,
    client_id: ClientId,
    expected_account: Option<&str>,
) -> AccountConnectionV1 {
    // Native callbacks can run while the engine is mutably borrowed. Do not
    // panic or infer connectivity from receiving a snapshot in that case.
    let Ok(engine) = engine.try_borrow() else {
        return AccountConnectionV1::Unknown;
    };
    let Some(client) = engine.get_client(&client_id) else {
        return AccountConnectionV1::Unknown;
    };
    if expected_account.is_some_and(|account| {
        client.account_id().to_string() != account || client.client_id() != client_id
    }) {
        return AccountConnectionV1::Unknown;
    }
    match (handle.state(), client.is_connected()) {
        (NodeState::Running, true) => AccountConnectionV1::Connected,
        (NodeState::Running | NodeState::Stopped, false) => AccountConnectionV1::Disconnected,
        _ => AccountConnectionV1::Unknown,
    }
}

fn retain(
    file: &mut File,
    envelope: &AccountObservationSubmitV1,
    native_client_id: Option<&str>,
) -> Result<()> {
    if let Some(client) = native_client_id {
        retained::retain_client_bound_envelope(
            file,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: client.to_owned(),
                observation: envelope.clone(),
            },
        )
    } else {
        retained::retain_envelope(file, envelope)
    }
}

impl NativeNodeObserver {
    /// Attach to one exact native account/client in a fresh node session.
    /// The segment is created exclusively and synced by a separate writer thread.
    pub fn attach(
        node: &LiveNode,
        binding: NativeAccountBindingV1,
        client_id: ClientId,
        output: &Path,
        queue_capacity: usize,
    ) -> Result<Self> {
        Self::attach_inner(node, binding, client_id, output, queue_capacity, false)
    }

    /// Opt-in client-bound observation producer. The host selects its project and
    /// an existing client; native account/trader/session/environment/version are
    /// read from the actual existing official node and execution client.
    /// This does not construct or connect a client and does not prove venue access.
    pub fn attach_client_bound(
        node: &LiveNode,
        project_id: Id,
        client_id: ClientId,
        output: &Path,
        queue_capacity: usize,
    ) -> Result<Self> {
        if node.state() != NodeState::Idle || queue_capacity == 0 {
            bail!("observer requires an idle node and a bounded nonzero queue");
        }
        let engine = node.kernel().exec_engine().clone();
        let view = engine
            .try_borrow()
            .map_err(|_| anyhow!("native execution engine is busy"))?;
        let client = view
            .get_client(&client_id)
            .ok_or_else(|| anyhow!("native execution client missing"))?;
        if client.client_id() != client_id {
            bail!("native execution client identity mismatch");
        }
        let account_id = client.account_id().to_string();
        drop(view);
        let environment = match node.environment() {
            Environment::Sandbox => ForwardEnvironmentV1::Paper,
            Environment::Live => ForwardEnvironmentV1::Live,
            _ => bail!("native node environment is not Paper or Live"),
        };
        let binding = NativeAccountBindingV1 {
            schema_version: SchemaV1,
            project_id,
            environment,
            native_trader_id: node.trader_id().to_string(),
            native_session_id: node.instance_id().to_string(),
            native_account_id: account_id,
            native_version: nautilus_core::consts::NAUTILUS_VERSION_CORE.to_owned(),
        };
        Self::attach_inner(node, binding, client_id, output, queue_capacity, true)
    }

    fn attach_inner(
        node: &LiveNode,
        binding: NativeAccountBindingV1,
        client_id: ClientId,
        output: &Path,
        queue_capacity: usize,
        client_bound: bool,
    ) -> Result<Self> {
        if node.state() != NodeState::Idle || queue_capacity == 0 {
            bail!("observer requires an idle node and a bounded nonzero queue");
        }
        let environment = match node.environment() {
            Environment::Sandbox => ForwardEnvironmentV1::Paper,
            Environment::Live => ForwardEnvironmentV1::Live,
            _ => bail!("native node environment is not Paper or Live"),
        };
        if binding.environment != environment
            || binding.native_trader_id != node.trader_id().to_string()
            || binding.native_session_id != node.instance_id().to_string()
        {
            bail!("observer binding does not match native node identity");
        }
        let engine = node.kernel().exec_engine().clone();
        {
            let engine = engine.borrow();
            let client = engine
                .get_client(&client_id)
                .ok_or_else(|| anyhow!("native execution client missing"))?;
            if client.account_id().to_string() != binding.native_account_id {
                bail!("observer client account binding mismatch");
            }
            if client_bound && client.client_id() != client_id {
                bail!("native execution client identity mismatch");
            }
        }
        let native_account_id = binding.native_account_id.clone();
        let native_client_id = client_bound.then(|| client_id.to_string());
        let topic = format!("events.portfolio.{}", binding.native_account_id);
        let observer = NativeAccountObserver::new(binding, DbCounter::ZERO, DbCounter::ZERO)?;
        let mut file = retained::new_segment(output)?;
        let (sender, receiver) = mpsc::sync_channel(queue_capacity);
        let retained_client = native_client_id.clone();
        let writer = std::thread::Builder::new()
            .name("native-account-retain".into())
            .spawn(move || {
                for envelope in receiver {
                    retain(&mut file, &envelope, retained_client.as_deref())?;
                }
                file.sync_all()?;
                Ok(file)
            })?;
        let handle = node.handle();
        let status_handle = handle.clone();
        let status_engine = engine.clone();
        let expected_account = client_bound.then(|| native_account_id.clone());
        let handler = observer.checked_handler(sender.clone(), retained::now, move || {
            connection(
                &status_handle,
                &status_engine,
                client_id,
                expected_account.as_deref(),
            )
        });
        msgbus::subscribe_portfolio_snapshot(topic.as_str().into(), handler.clone(), None);
        Ok(Self {
            observer,
            sender: Some(sender),
            handler: Some(handler),
            topic,
            handle,
            engine,
            client_id,
            native_client_id,
            native_account_id,
            writer: Some(writer),
        })
    }

    /// Host lifecycle observation, separate from native valuation freshness.
    /// False means bounded queue loss; cursor/drop count records that fact.
    pub fn heartbeat(&self) -> Result<bool> {
        if self.writer.as_ref().is_some_and(JoinHandle::is_finished) {
            bail!(
                "native observation writer stopped; retain segment and restart with a new session"
            );
        }
        self.observer.try_emit(
            None,
            retained::now()?,
            connection(
                &self.handle,
                &self.engine,
                self.client_id,
                self.native_client_id
                    .as_ref()
                    .map(|_| self.native_account_id.as_str()),
            ),
            self.sender
                .as_ref()
                .ok_or_else(|| anyhow!("observer detached"))?,
        )
    }

    pub fn cursor(&self) -> (DbCounter, DbCounter) {
        self.observer.cursor()
    }

    fn detach(&mut self) {
        if let Some(handler) = self.handler.take() {
            msgbus::unsubscribe_portfolio_snapshot(self.topic.as_str().into(), &handler);
        }
        self.sender.take();
    }

    /// Drain and sync all accepted frames, then append a terminal lifecycle frame.
    /// A final frame carries any queue loss even when there was no later snapshot.
    /// This may perform blocking I/O, so only call after native lifecycle completion.
    pub fn finish(mut self) -> Result<(DbCounter, DbCounter)> {
        if matches!(
            self.handle.state(),
            NodeState::Starting | NodeState::Running | NodeState::ShuttingDown
        ) {
            bail!("finish observer only after native shutdown");
        }
        let status = connection(
            &self.handle,
            &self.engine,
            self.client_id,
            self.native_client_id
                .as_ref()
                .map(|_| self.native_account_id.as_str()),
        );
        self.detach();
        let mut file = self
            .writer
            .take()
            .ok_or_else(|| anyhow!("observer writer missing"))?
            .join()
            .map_err(|_| anyhow!("native observation writer panicked; retain segment"))??;
        // Native dispatch has stopped and the writer has drained. This one-place
        // queue cannot overflow; reuse the exact existing envelope projection.
        let (sender, receiver) = mpsc::sync_channel(1);
        self.observer
            .try_emit(None, retained::now()?, status, &sender)?;
        retain(
            &mut file,
            &receiver.recv()?,
            self.native_client_id.as_deref(),
        )?;
        file.sync_all()?;
        Ok(self.observer.cursor())
    }
}

impl Drop for NativeNodeObserver {
    fn drop(&mut self) {
        self.detach();
    }
}
