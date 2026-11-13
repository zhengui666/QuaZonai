//! Internal typed account seam, not a wire command or production connection.
//!
//! A user factory is trusted Rust code, not a sandbox. Even constructing a builder
//! can have arbitrary effects. Calling it requires an explicit execution request;
//! neither capability inspection nor recovery may call it. The existing tested
//! account recovery path is deliberately not generalized through this module.
use anyhow::{Result, bail};
use nautilus_common::{
    actor::{DataActor, DataActorConfig, DataActorCore},
    nautilus_actor,
    timer::TimeEvent,
};
use nautilus_core::DurationNanos;
use nautilus_live::node::LiveNodeBuilder;
use nautilus_model::identifiers::StrategyId;
use nautilus_system::Controller;
use serde_json::Value;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

/// Uses the complete official builder without copying its configuration schema.
/// The owner chooses official clients, routing and engine configuration in Rust.
pub type AccountNodeFactory = fn(&Value) -> Result<LiveNodeBuilder>;

/// Runs on the node-owning thread only, after an explicit strategy request.
/// Implementations use the official Controller and return actual created IDs.
/// This callback is trusted code too; it is not proof that strategies stay stopped.
pub type AccountComponentsFactory = fn(&Controller, &str, &Value) -> Result<Vec<StrategyId>>;

#[derive(Clone, Copy, Debug)]
pub enum Preparation {
    ExplicitExecution,
    Recovery,
}

/// Prepare only: does not build, connect, run, register or start a node itself.
/// An explicit caller must still own the correct identity, session and permission.
pub fn prepare_builder(
    factory: Option<AccountNodeFactory>,
    preparation: Preparation,
    config: &Value,
) -> Result<LiveNodeBuilder> {
    if matches!(preparation, Preparation::Recovery) {
        bail!("NATIVE_CONFIGURED_ACCOUNT_RECOVERY_UNSUPPORTED");
    }
    let Some(factory) = factory else {
        bail!("NATIVE_CONFIGURED_ACCOUNT_REQUIRES_USER_FACTORY");
    };
    factory(config)
}

/// One real upstream ID per operation. No broadcast success or QZ strategy state.
/// Must be dispatched by the existing node-owning actor after explicit admission.
#[derive(Clone, Copy, Debug)]
pub enum StrategyCommand {
    Start,
    Stop,
}

pub fn control_strategy(
    controller: &Controller,
    id: &StrategyId,
    command: StrategyCommand,
) -> Result<()> {
    match command {
        StrategyCommand::Start => controller.start_strategy(id),
        StrategyCommand::Stop => controller.stop_strategy(id),
    }
}

/// Already-admitted transport request. No native session is started by enqueueing.
#[derive(Debug)]
pub struct StrategyControlRequest {
    pub request_id: String,
    pub strategy_id: StrategyId,
    pub command: StrategyCommand,
}

#[derive(Debug)]
pub struct StrategyControlReply {
    pub request_id: String,
    pub strategy_id: StrategyId,
    pub result: Result<()>,
}

/// Native-thread dispatch only, not a second request ledger or retry mechanism.
#[derive(Debug)]
pub struct AccountControlActor {
    core: DataActorCore,
    controller: Controller,
    inbox: Receiver<StrategyControlRequest>,
    replies: SyncSender<StrategyControlReply>,
}
nautilus_actor!(AccountControlActor);

impl AccountControlActor {
    fn dispatch_pending(&mut self) -> Result<()> {
        for _ in 0..32 {
            let Ok(request) = self.inbox.try_recv() else {
                break;
            };
            let result = control_strategy(&self.controller, &request.strategy_id, request.command);
            self.replies
                .try_send(StrategyControlReply {
                    request_id: request.request_id,
                    strategy_id: request.strategy_id,
                    result,
                })
                .map_err(|error| {
                    anyhow::anyhow!("native account control reply unavailable: {error}")
                })?;
        }
        Ok(())
    }
    pub fn channel(
        config: DataActorConfig,
        controller: Controller,
    ) -> (
        Self,
        SyncSender<StrategyControlRequest>,
        Receiver<StrategyControlReply>,
    ) {
        let (sender, inbox) = sync_channel(64);
        let (replies, receiver) = sync_channel(64);
        (
            Self {
                core: DataActorCore::new(config),
                controller,
                inbox,
                replies,
            },
            sender,
            receiver,
        )
    }
}

impl DataActor for AccountControlActor {
    fn on_start(&mut self) -> Result<()> {
        self.clock().set_timer_ns(
            "qz-typed-account-input",
            DurationNanos::new(20_000_000),
            None,
            None,
            None,
            Some(false),
            Some(false),
        )?;
        Ok(())
    }

    fn on_time_event(&mut self, _: &TimeEvent) -> Result<()> {
        // If delivery fails, execution may already have happened. The transport
        // must preserve UNKNOWN, never enqueue a retry.
        self.dispatch_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_common::{actor::DataActor, enums::Environment, logging::logger::LoggerConfig};
    use nautilus_live::node::LiveNode;
    use nautilus_model::identifiers::TraderId;
    use nautilus_trading::{
        nautilus_strategy,
        strategy::{StrategyConfig, StrategyCore},
    };
    use std::{cell::RefCell, rc::Rc};

    #[derive(Debug)]
    struct Lifecycle {
        core: StrategyCore,
        events: Rc<RefCell<Vec<String>>>,
        label: String,
    }
    nautilus_strategy!(Lifecycle, {});
    impl DataActor for Lifecycle {
        fn on_start(&mut self) -> Result<()> {
            self.events
                .borrow_mut()
                .push(format!("{}:start", self.label));
            Ok(())
        }
        fn on_stop(&mut self) -> Result<()> {
            self.events
                .borrow_mut()
                .push(format!("{}:stop", self.label));
            Ok(())
        }
    }

    fn must_not_execute(_: &Value) -> Result<LiveNodeBuilder> {
        panic!("recovery must reject before any trusted user code is invoked")
    }

    #[test]
    fn arbitrary_factory_recovery_is_rejected_before_callback() {
        let error = prepare_builder(Some(must_not_execute), Preparation::Recovery, &Value::Null)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "NATIVE_CONFIGURED_ACCOUNT_RECOVERY_UNSUPPORTED"
        );
    }

    #[test]
    fn explicit_execution_requires_a_factory() {
        let error =
            prepare_builder(None, Preparation::ExplicitExecution, &Value::Null).unwrap_err();
        assert_eq!(
            error.to_string(),
            "NATIVE_CONFIGURED_ACCOUNT_REQUIRES_USER_FACTORY"
        );
    }

    #[test]
    fn bounded_queue_rejects_unsent_request_and_drop_does_not_execute_it() -> Result<()> {
        let node = LiveNode::builder(TraderId::from("BOUND-001"), Environment::Sandbox)?
            .with_logging(LoggerConfig {
                bypass_logging: true,
                ..Default::default()
            })
            .build()?;
        let (actor, commands, replies) = AccountControlActor::channel(
            DataActorConfig::default(),
            Controller::new(node.kernel().trader.clone(), None),
        );
        for index in 0..64 {
            commands.try_send(StrategyControlRequest {
                request_id: index.to_string(),
                strategy_id: StrategyId::from("MISSING-001"),
                command: StrategyCommand::Start,
            })?;
        }
        let rejected = commands.try_send(StrategyControlRequest {
            request_id: "not-enqueued".into(),
            strategy_id: StrategyId::from("MISSING-001"),
            command: StrategyCommand::Start,
        });
        assert!(matches!(
            rejected,
            Err(std::sync::mpsc::TrySendError::Full(_))
        ));
        // Teardown with admitted work still in flight provides no completion
        // receipt. Outer durable tasks must not infer success or retry it.
        drop(actor);
        assert!(matches!(
            replies.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Disconnected)
        ));
        Ok(())
    }

    #[test]
    fn lost_or_full_reply_can_follow_successful_execution() -> Result<()> {
        let node = LiveNode::builder(TraderId::from("REPLY-001"), Environment::Sandbox)?
            .with_logging(LoggerConfig {
                bypass_logging: true,
                ..Default::default()
            })
            .build()?;
        let controller = Controller::new(node.kernel().trader.clone(), None);
        let events = Rc::new(RefCell::new(Vec::new()));
        for (name, tag, disconnected) in [("LOST-001", "001", true), ("FULL-002", "002", false)] {
            let id = controller.create_strategy(
                Lifecycle {
                    core: StrategyCore::new(StrategyConfig {
                        strategy_id: Some(StrategyId::from(name)),
                        order_id_tag: Some(tag.into()),
                        ..Default::default()
                    }),
                    events: events.clone(),
                    label: name.into(),
                },
                false,
            )?;
            let (mut actor, commands, replies) = AccountControlActor::channel(
                DataActorConfig::default(),
                Controller::new(node.kernel().trader.clone(), None),
            );
            let retained = if disconnected {
                drop(replies);
                None
            } else {
                for index in 0..64 {
                    actor.replies.try_send(StrategyControlReply {
                        request_id: format!("prior-{index}"),
                        strategy_id: id,
                        result: Ok(()),
                    })?;
                }
                Some(replies)
            };
            commands.try_send(StrategyControlRequest {
                request_id: format!("request-{name}"),
                strategy_id: id,
                command: StrategyCommand::Start,
            })?;
            assert!(actor.dispatch_pending().is_err());
            assert!(events.borrow().contains(&format!("{name}:start")));
            // Dispatch cannot replay the consumed request on the next tick.
            let before = events.borrow().len();
            actor.dispatch_pending()?;
            assert_eq!(events.borrow().len(), before);
            drop(retained);
            controller.stop_strategy(&id)?;
        }
        Ok(())
    }

    #[test]
    fn official_controller_targets_distinct_ids_and_retains_individual_error() -> Result<()> {
        // No exchange clients, prices or orders. This verifies component lifecycle
        // on its owning thread, not a connected multi-venue session or recovery.
        let node = LiveNode::builder(TraderId::from("TYPED-001"), Environment::Sandbox)?
            .with_logging(LoggerConfig {
                bypass_logging: true,
                ..Default::default()
            })
            .build()?;
        let controller = Controller::new(node.kernel().trader.clone(), None);
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut ids = Vec::new();
        for (name, tag) in [("FIRST-001", "001"), ("SECOND-002", "002")] {
            ids.push(controller.create_strategy(
                Lifecycle {
                    core: StrategyCore::new(StrategyConfig {
                        strategy_id: Some(StrategyId::from(name)),
                        order_id_tag: Some(tag.into()),
                        ..Default::default()
                    }),
                    events: events.clone(),
                    label: name.to_owned(),
                },
                false,
            )?);
        }
        assert!(events.borrow().is_empty());
        control_strategy(&controller, &ids[0], StrategyCommand::Start)?;
        assert!(
            control_strategy(
                &controller,
                &StrategyId::from("MISSING-003"),
                StrategyCommand::Start
            )
            .is_err()
        );
        assert_eq!(*events.borrow(), vec!["FIRST-001:start"]);
        control_strategy(&controller, &ids[1], StrategyCommand::Start)?;
        control_strategy(&controller, &ids[0], StrategyCommand::Stop)?;
        assert_eq!(
            *events.borrow(),
            vec!["FIRST-001:start", "SECOND-002:start", "FIRST-001:stop"]
        );
        control_strategy(&controller, &ids[1], StrategyCommand::Stop)?;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn running_node_dispatches_only_explicit_queued_ids() -> Result<()> {
        let mut node = LiveNode::builder(TraderId::from("QUEUED-001"), Environment::Sandbox)?
            .with_logging(LoggerConfig {
                bypass_logging: true,
                ..Default::default()
            })
            .with_delay_post_stop_secs(0)
            .with_delay_shutdown_secs(0)
            .build()?;
        let controller = Controller::new(node.kernel().trader.clone(), None);
        let (actor, commands, replies) = AccountControlActor::channel(
            DataActorConfig::default(),
            Controller::new(node.kernel().trader.clone(), None),
        );
        node.add_actor(actor)?;
        let handle = node.handle();
        let events = Rc::new(RefCell::new(Vec::new()));
        let exercise = async {
            while !handle.is_running() {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
            assert!(events.borrow().is_empty());
            // Register only AFTER the node has started; preloading would let
            // upstream node startup invoke on_start without a control request.
            let mut ids = Vec::new();
            for (name, tag) in [("QUEUE-A-001", "001"), ("QUEUE-B-002", "002")] {
                ids.push(controller.create_strategy(
                    Lifecycle {
                        core: StrategyCore::new(StrategyConfig {
                            strategy_id: Some(StrategyId::from(name)),
                            order_id_tag: Some(tag.into()),
                            ..Default::default()
                        }),
                        events: events.clone(),
                        label: name.to_owned(),
                    },
                    false,
                )?);
            }
            assert!(events.borrow().is_empty());
            let cases = [
                ("start-a", ids[0], StrategyCommand::Start, true),
                (
                    "missing",
                    StrategyId::from("QUEUE-MISSING-003"),
                    StrategyCommand::Start,
                    false,
                ),
                // The actor deliberately has no durable dedupe: an outer layer
                // must never replay this ID. Both admitted commands execute here.
                ("start-a", ids[1], StrategyCommand::Start, true),
                ("stop-a", ids[0], StrategyCommand::Stop, true),
                ("stop-b", ids[1], StrategyCommand::Stop, true),
            ];
            for (request_id, strategy_id, command, success) in cases {
                commands.try_send(StrategyControlRequest {
                    request_id: request_id.into(),
                    strategy_id,
                    command,
                })?;
                let reply = loop {
                    if let Ok(reply) = replies.try_recv() {
                        break reply;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                };
                assert_eq!(reply.request_id, request_id);
                assert_eq!(reply.strategy_id, strategy_id);
                assert_eq!(reply.result.is_ok(), success);
            }
            assert_eq!(
                *events.borrow(),
                vec![
                    "QUEUE-A-001:start",
                    "QUEUE-B-002:start",
                    "QUEUE-A-001:stop",
                    "QUEUE-B-002:stop"
                ]
            );
            handle.stop();
            Ok::<_, anyhow::Error>(())
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::try_join!(node.run(), exercise)
        })
        .await;
        handle.stop();
        result??;
        Ok(())
    }
}
