//! Owns only a bounded native account-operation connection. The official child
//! performs device-code authorization; this module never calls OAuth endpoints.
use super::*;
use contracts::codex::{CodexAccountActionV1, CodexAccountReasonV1 as Reason, CodexDeviceCodeV1};
use store::codex_profiles::account::{
    CodexAccountCompletion as Completion, CodexAccountNext, CodexAccountTicket,
};
use tokio::sync::oneshot;

pub(super) struct AccountMemory {
    id: contracts::Id,
    device: Option<CodexDeviceCodeV1>,
}

impl CodexDeployment {
    /// A same-key POST can redisplay the still-live one-time code, but only after
    /// Store reauthorizes its original immutable acceptance. GET never calls this.
    pub async fn device_code(&self, id: contracts::Id) -> Option<CodexDeviceCodeV1> {
        for binding in self.bindings.values() {
            let memory = binding.account.lock().await;
            if let Some(active) = memory.as_ref().filter(|active| active.id == id) {
                return active.device.clone();
            }
        }
        None
    }

    pub async fn owns_account_operation(&self, id: contracts::Id) -> bool {
        for binding in self.bindings.values() {
            if binding
                .account
                .lock()
                .await
                .as_ref()
                .is_some_and(|active| active.id == id)
            {
                return true;
            }
        }
        false
    }

    /// Only a newly committed acceptance supplies a ticket. A replay does not
    /// launch another child or resend an account mutation.
    pub async fn start_account(&self, store: store::Store, ticket: Box<CodexAccountTicket>) {
        let Some(reference) = ticket.snapshot.profile.home_binding.as_deref() else {
            let _ = store
                .complete_codex_account(&ticket, Completion::Failed(Reason::DeploymentUnavailable))
                .await;
            return;
        };
        let binding = match self.binding(reference, ticket.snapshot.profile.profile_origin) {
            Ok(binding) => binding,
            Err(_) => {
                let _ = store
                    .complete_codex_account(
                        &ticket,
                        Completion::Failed(Reason::DeploymentUnavailable),
                    )
                    .await;
                return;
            }
        };
        let guard = match binding.gate.clone().try_lock_owned() {
            Ok(guard) => guard,
            Err(_) => {
                let _ = store
                    .complete_codex_account(
                        &ticket,
                        Completion::Failed(Reason::DeploymentUnavailable),
                    )
                    .await;
                return;
            }
        };
        let launch = self.launch(binding, &binding.working_directory);
        let memory = binding.account.clone();
        let id = ticket.acceptance.resource.id;
        *memory.lock().await = Some(AccountMemory { id, device: None });
        let (ready, waiting) = oneshot::channel();
        tokio::spawn(async move {
            // The held native binding lock, client and code all have one bounded
            // owner. RPC timeouts do not silently create a replacement process.
            let _guard = guard;
            let result = tokio::time::timeout(
                Duration::from_secs(930),
                drive(&store, &ticket, launch, &memory, ready),
            )
            .await;
            if !matches!(result, Ok(Ok(()))) {
                let reason = if result.is_err() {
                    Reason::WaitWindowEnded
                } else {
                    Reason::NativeResponseUnknown
                };
                let _ = store
                    .complete_codex_account(&ticket, Completion::Unknown(reason))
                    .await;
            }
            let mut current = memory.lock().await;
            if current.as_ref().is_some_and(|active| active.id == id) {
                *current = None;
            }
        });
        // Acceptance remains 202 when the bounded initial response is not ready.
        // The native owner continues independently of the browser connection.
        let _ = tokio::time::timeout(Duration::from_secs(25), waiting).await;
    }
}

fn before_send(error: native::NativeFailure) -> Completion {
    let reason = match error {
        native::NativeFailure::Version => Reason::NativeVersionUnsupported,
        native::NativeFailure::Contract
        | native::NativeFailure::FrameLimit
        | native::NativeFailure::Correlation
        | native::NativeFailure::ObservationLimit => Reason::NativeContractUnsupported,
        _ => Reason::DeploymentUnavailable,
    };
    Completion::Failed(reason)
}

fn after_send(error: native::NativeFailure) -> Completion {
    match error {
        // A rejected status/read/cancel after a send does not prove that the
        // earlier account mutation did not take effect.
        native::NativeFailure::Rejected(_) => Completion::Unknown(Reason::NativeResponseUnknown),
        native::NativeFailure::Contract
        | native::NativeFailure::FrameLimit
        | native::NativeFailure::Correlation
        | native::NativeFailure::ObservationLimit => {
            Completion::Unknown(Reason::NativeContractUnsupported)
        }
        _ => Completion::Unknown(Reason::NativeResponseUnknown),
    }
}

async fn drive(
    store: &store::Store,
    ticket: &CodexAccountTicket,
    launch: Launch,
    memory: &Arc<Mutex<Option<AccountMemory>>>,
    ready: oneshot::Sender<()>,
) -> Result<(), StoreError> {
    let mut client = match Client::start(launch).await {
        Ok(client) => client,
        Err(error) => {
            store
                .complete_codex_account(ticket, before_send(error))
                .await?;
            return Ok(());
        }
    };
    let result = connected(store, ticket, &mut client, memory, ready).await;
    // Closing cannot turn an unconfirmed native mutation into confirmed failure.
    let _ = client.close().await;
    match result {
        Ok(Some(completion)) => {
            store.complete_codex_account(ticket, completion).await?;
        }
        Ok(None) => {}
        Err(error) => return Err(error),
    }
    Ok(())
}

async fn connected(
    store: &store::Store,
    ticket: &CodexAccountTicket,
    client: &mut Client,
    memory: &Arc<Mutex<Option<AccountMemory>>>,
    ready: oneshot::Sender<()>,
) -> Result<Option<Completion>, StoreError> {
    if !store.begin_codex_account(ticket).await? {
        return Ok(None);
    }
    if ticket.acceptance.resource.action == CodexAccountActionV1::Logout {
        if let Err(error) = client.logout().await {
            return Ok(Some(after_send(error)));
        }
        return Ok(Some(match client.account().await {
            Ok(account) => Completion::LoggedOut(account_view(account)),
            Err(error) => after_send(error),
        }));
    }
    let code = match client.device_login().await {
        Ok(code) => code,
        Err(error) => return Ok(Some(after_send(error))),
    };
    let native_id = code.login_id;
    store.bind_codex_login(ticket, &native_id).await?;
    {
        let mut memory = memory.lock().await;
        let active = memory
            .as_mut()
            .filter(|active| active.id == ticket.acceptance.resource.id)
            .ok_or(StoreError::Integrity)?;
        active.device = Some(CodexDeviceCodeV1 {
            verification_url: code.verification_url,
            user_code: code.user_code,
        });
    }
    let _ = ready.send(());
    loop {
        match store.next_codex_account(ticket).await? {
            CodexAccountNext::Finished => return Ok(None),
            CodexAccountNext::Cancel(id) => {
                if id != native_id {
                    return Err(StoreError::Integrity);
                }
                let cancelled = match client.cancel_login(&id).await {
                    Ok(value) => value.status,
                    Err(error) => return Ok(Some(after_send(error))),
                };
                // A completion already received while awaiting the cancellation
                // reply wins over the later request to stop that same login.
                match client.observations(Duration::ZERO).await {
                    Ok(observations) if observations.iter().any(|value| matches!(value,
                        native::Observation::LoginCompleted { login_id, success: true } if login_id == &native_id)) => {
                        return Ok(Some(login_completed(client).await));
                    }
                    Err(error) => return Ok(Some(after_send(error))),
                    _ => {}
                }
                return Ok(Some(match cancelled {
                    native::LoginCancellationStatus::Canceled => Completion::Cancelled,
                    native::LoginCancellationStatus::NotFound => {
                        Completion::Unknown(Reason::NativeResponseUnknown)
                    }
                }));
            }
            CodexAccountNext::Wait => {}
        }
        let observed = match client.observations(Duration::from_secs(1)).await {
            Ok(value) => value,
            Err(error) => return Ok(Some(after_send(error))),
        };
        for value in observed {
            if let native::Observation::LoginCompleted { login_id, success } = value {
                if login_id != native_id {
                    continue;
                }
                return Ok(Some(if success {
                    login_completed(client).await
                } else {
                    Completion::Failed(Reason::NativeLoginRejected)
                }));
            }
        }
    }
}

async fn login_completed(client: &mut Client) -> Completion {
    match client.account().await {
        Ok(value) => {
            let account = account_view(value);
            if account.authentication_kind == Some(CodexAuthenticationKind::Chatgpt) {
                Completion::LoggedIn(account)
            } else {
                Completion::Unknown(Reason::NativeContractUnsupported)
            }
        }
        Err(error) => after_send(error),
    }
}
