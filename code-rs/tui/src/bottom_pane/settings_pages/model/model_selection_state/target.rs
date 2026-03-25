use crate::app_event::{AppEvent, ModelSelectionKind};
use crate::app_event_sender::AppEventSender;

use super::data::SelectionAction;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModelSelectionTarget {
    Session,
    Review,
    Planning,
    AutoDrive,
    ReviewResolve,
    AutoReview,
    AutoReviewResolve,
}

impl From<ModelSelectionTarget> for ModelSelectionKind {
    fn from(target: ModelSelectionTarget) -> Self {
        match target {
            ModelSelectionTarget::Session => ModelSelectionKind::Session,
            ModelSelectionTarget::Review => ModelSelectionKind::Review,
            ModelSelectionTarget::Planning => ModelSelectionKind::Planning,
            ModelSelectionTarget::AutoDrive => ModelSelectionKind::AutoDrive,
            ModelSelectionTarget::ReviewResolve => ModelSelectionKind::ReviewResolve,
            ModelSelectionTarget::AutoReview => ModelSelectionKind::AutoReview,
            ModelSelectionTarget::AutoReviewResolve => ModelSelectionKind::AutoReviewResolve,
        }
    }
}

impl ModelSelectionTarget {
    pub(crate) fn panel_title(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Select Model & Reasoning",
            ModelSelectionTarget::Review => "Select Review Model & Reasoning",
            ModelSelectionTarget::Planning => "Select Planning Model & Reasoning",
            ModelSelectionTarget::AutoDrive => "Select Auto Drive Model & Reasoning",
            ModelSelectionTarget::ReviewResolve => "Select Resolve Model & Reasoning",
            ModelSelectionTarget::AutoReview => "Select Auto Review Model & Reasoning",
            ModelSelectionTarget::AutoReviewResolve => {
                "Select Auto Review Resolve Model & Reasoning"
            }
        }
    }

    pub(crate) fn current_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Current model",
            ModelSelectionTarget::Review => "Review model",
            ModelSelectionTarget::Planning => "Planning model",
            ModelSelectionTarget::AutoDrive => "Auto Drive model",
            ModelSelectionTarget::ReviewResolve => "Resolve model",
            ModelSelectionTarget::AutoReview => "Auto Review model",
            ModelSelectionTarget::AutoReviewResolve => "Auto Review resolve model",
        }
    }

    pub(crate) fn reasoning_label(self) -> &'static str {
        match self {
            ModelSelectionTarget::Session => "Reasoning effort",
            ModelSelectionTarget::Review => "Review reasoning",
            ModelSelectionTarget::Planning => "Planning reasoning",
            ModelSelectionTarget::AutoDrive => "Auto Drive reasoning",
            ModelSelectionTarget::ReviewResolve => "Resolve reasoning",
            ModelSelectionTarget::AutoReview => "Auto Review reasoning",
            ModelSelectionTarget::AutoReviewResolve => "Auto Review resolve reasoning",
        }
    }

    pub(crate) fn supports_follow_chat(self) -> bool {
        !matches!(self, ModelSelectionTarget::Session)
    }

    pub(crate) fn supports_fast_mode(self, current_model: &str) -> bool {
        matches!(self, ModelSelectionTarget::Session)
            && code_core::model_family::supports_service_tier(current_model)
    }

    pub(crate) fn supports_context_mode(self) -> bool {
        matches!(self, ModelSelectionTarget::Session)
    }

    pub(crate) fn dispatch_selection_action(
        self,
        app_event_tx: &AppEventSender,
        action: &SelectionAction,
    ) {
        match action {
            SelectionAction::ToggleFastMode(service_tier) => {
                // Fast mode is session-global, not target-specific.
                app_event_tx.send(AppEvent::UpdateServiceTierSelection {
                    service_tier: *service_tier,
                });
            }
            SelectionAction::SetContextMode(context_mode) => {
                // Context mode is session-global, not target-specific.
                app_event_tx.send(AppEvent::UpdateSessionContextModeSelection {
                    context_mode: *context_mode,
                });
            }
            SelectionAction::UseChatModel => match self {
                ModelSelectionTarget::Session => {}
                ModelSelectionTarget::Review => {
                    app_event_tx.send(AppEvent::UpdateReviewUseChatModel(true));
                }
                ModelSelectionTarget::Planning => {
                    app_event_tx.send(AppEvent::UpdatePlanningUseChatModel(true));
                }
                ModelSelectionTarget::AutoDrive => {
                    app_event_tx.send(AppEvent::UpdateAutoDriveUseChatModel(true));
                }
                ModelSelectionTarget::ReviewResolve => {
                    app_event_tx.send(AppEvent::UpdateReviewResolveUseChatModel(true));
                }
                ModelSelectionTarget::AutoReview => {
                    app_event_tx.send(AppEvent::UpdateAutoReviewUseChatModel(true));
                }
                ModelSelectionTarget::AutoReviewResolve => {
                    app_event_tx.send(AppEvent::UpdateAutoReviewResolveUseChatModel(true));
                }
            },
            SelectionAction::SetPreset { model, effort } => match self {
                ModelSelectionTarget::Session => {
                    app_event_tx.send(AppEvent::UpdateModelSelection {
                        model: model.clone(),
                        effort: Some(*effort),
                    });
                }
                ModelSelectionTarget::Review => {
                    app_event_tx.send(AppEvent::UpdateReviewModelSelection {
                        model: model.clone(),
                        effort: *effort,
                    });
                }
                ModelSelectionTarget::Planning => {
                    app_event_tx.send(AppEvent::UpdatePlanningModelSelection {
                        model: model.clone(),
                        effort: *effort,
                    });
                }
                ModelSelectionTarget::AutoDrive => {
                    app_event_tx.send(AppEvent::UpdateAutoDriveModelSelection {
                        model: model.clone(),
                        effort: *effort,
                    });
                }
                ModelSelectionTarget::ReviewResolve => {
                    app_event_tx.send(AppEvent::UpdateReviewResolveModelSelection {
                        model: model.clone(),
                        effort: *effort,
                    });
                }
                ModelSelectionTarget::AutoReview => {
                    app_event_tx.send(AppEvent::UpdateAutoReviewModelSelection {
                        model: model.clone(),
                        effort: *effort,
                    });
                }
                ModelSelectionTarget::AutoReviewResolve => {
                    app_event_tx.send(AppEvent::UpdateAutoReviewResolveModelSelection {
                        model: model.clone(),
                        effort: *effort,
                    });
                }
            },
        }
    }
}
