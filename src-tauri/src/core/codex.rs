use std::path::PathBuf;
use std::sync::Arc;

use async_channel::{Receiver, Sender};
use tokio::sync::Mutex;

use crate::config::ConfigLayerStack;
use crate::protocol::error::CodexError;
use crate::protocol::event::{Event, EventMsg, SessionConfiguredEvent};
use crate::protocol::submission::{Op, Submission};

use super::session::Session;

/// Core engine that processes the SQ/EQ loop.
pub struct Codex {
    /// Submission queue receiver (client → engine).
    sq_rx: Receiver<Submission>,
    /// Event queue sender (engine → client).
    eq_tx: Sender<Event>,
    /// Active session.
    session: Arc<Mutex<Option<Session>>>,
    /// Configuration stack.
    config: Arc<Mutex<ConfigLayerStack>>,
    /// Working directory.
    cwd: PathBuf,
    /// Whether the engine is running.
    running: Arc<Mutex<bool>>,
}

impl Codex {
    pub fn new(
        sq_rx: Receiver<Submission>,
        eq_tx: Sender<Event>,
        config: ConfigLayerStack,
        cwd: PathBuf,
    ) -> Self {
        Self {
            sq_rx,
            eq_tx,
            session: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(config)),
            cwd,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the SQ/EQ processing loop.
    pub async fn run(&self) -> Result<(), CodexError> {
        {
            let mut running = self.running.lock().await;
            *running = true;
        }

        // Emit SessionConfigured on startup
        let config = self.config.lock().await.merge();
        let session = Session::new(self.cwd.clone(), config.clone());
        let session_id = session.id().to_string();

        self.emit(EventMsg::SessionConfigured(SessionConfiguredEvent {
            session_id: session_id.clone(),
            forked_from_id: None,
            thread_name: None,
            model: config.model.clone().unwrap_or_else(|| "default".into()),
            model_provider_id: config.model_provider.clone().unwrap_or_default(),
            approval_policy: config.approval_policy.clone(),
            sandbox_policy: None,
            cwd: self.cwd.clone(),
            history_log_id: 0,
            history_entry_count: 0,
        }))
        .await;

        {
            let mut s = self.session.lock().await;
            *s = Some(session);
        }

        // Main loop: process submissions
        while let Ok(submission) = self.sq_rx.recv().await {
            let is_shutdown = matches!(&submission.op, Op::Shutdown);
            if let Err(e) = self.handle_op(submission.id, submission.op).await {
                self.emit(EventMsg::Error(crate::protocol::event::ErrorEvent {
                    message: e.message,
                    codex_error_info: None,
                }))
                .await;
            }
            if is_shutdown || !*self.running.lock().await {
                break;
            }
        }

        Ok(())
    }

    /// Stop the engine.
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
    }

    /// Dispatch a single Op.
    async fn handle_op(&self, _id: String, op: Op) -> Result<(), CodexError> {
        match op {
            Op::Interrupt => {
                let mut session = self.session.lock().await;
                if let Some(s) = session.as_mut() {
                    s.interrupt();
                }
            }
            Op::Shutdown => {
                self.stop().await;
                self.emit(EventMsg::ShutdownComplete).await;
            }
            Op::UserTurn { .. } => {
                let mut session = self.session.lock().await;
                if let Some(s) = session.as_mut() {
                    let turn_id = s.start_turn();
                    self.emit(EventMsg::TurnStarted(
                        crate::protocol::event::TurnStartedEvent {
                            turn_id: turn_id.clone(),
                            model_context_window: None,
                            collaboration_mode_kind: crate::protocol::types::ModeKind::Default,
                        },
                    ))
                    .await;

                    // TODO: dispatch to tool handlers, model API, etc.

                    self.emit(EventMsg::TurnComplete(
                        crate::protocol::event::TurnCompleteEvent {
                            turn_id,
                            last_agent_message: None,
                        },
                    ))
                    .await;
                }
            }
            Op::UserInput { .. } => {
                // Legacy path — same as UserTurn but simplified
            }
            Op::OverrideTurnContext {
                model,
                ..
            } => {
                let mut session = self.session.lock().await;
                if let Some(s) = session.as_mut() {
                    if let Some(m) = model {
                        s.set_model(m);
                    }
                }
            }
            Op::Compact => {
                // TODO: invoke context compaction
                self.emit(EventMsg::ContextCompacted(
                    crate::protocol::event::ContextCompactedEvent,
                ))
                .await;
            }
            Op::Undo => {
                self.emit(EventMsg::UndoStarted(
                    crate::protocol::event::UndoStartedEvent { message: None },
                ))
                .await;
                // TODO: actual undo logic
                self.emit(EventMsg::UndoCompleted(
                    crate::protocol::event::UndoCompletedEvent {
                        success: true,
                        message: None,
                    },
                ))
                .await;
            }
            Op::ReloadUserConfig => {
                // TODO: reload config from disk
            }
            Op::ListMcpTools => {
                self.emit(EventMsg::McpListToolsResponse(
                    crate::protocol::event::McpListToolsResponseEvent {
                        tools: std::collections::HashMap::new(),
                    },
                ))
                .await;
            }
            Op::ListSkills { .. } => {
                self.emit(EventMsg::ListSkillsResponse(
                    crate::protocol::event::ListSkillsResponseEvent { skills: vec![] },
                ))
                .await;
            }
            Op::ListCustomPrompts => {
                self.emit(EventMsg::ListCustomPromptsResponse(
                    crate::protocol::event::ListCustomPromptsResponseEvent {
                        custom_prompts: vec![],
                    },
                ))
                .await;
            }
            _ => {
                // Other ops: log and skip for now
            }
        }
        Ok(())
    }

    async fn emit(&self, msg: EventMsg) {
        let event = Event {
            id: uuid::Uuid::new_v4().to_string(),
            msg,
        };
        let _ = self.eq_tx.send(event).await;
    }

    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigLayerStack;

    #[tokio::test]
    async fn codex_start_emits_session_configured() {
        let (sq_tx, sq_rx) = async_channel::unbounded();
        let (eq_tx, eq_rx) = async_channel::unbounded();
        let codex = Codex::new(
            sq_rx,
            eq_tx,
            ConfigLayerStack::new(),
            std::env::current_dir().unwrap(),
        );

        // Send shutdown immediately so the loop exits
        sq_tx
            .send(Submission {
                id: "s1".into(),
                op: Op::Shutdown,
            })
            .await
            .unwrap();

        codex.run().await.unwrap();

        let mut events = vec![];
        while let Ok(ev) = eq_rx.try_recv() {
            events.push(ev);
        }
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::SessionConfigured(_))));
        assert!(events
            .iter()
            .any(|e| matches!(&e.msg, EventMsg::ShutdownComplete)));
    }
}
