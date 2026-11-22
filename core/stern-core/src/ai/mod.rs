pub mod chat;
pub mod intent;
pub mod model;
pub mod model_manager;

pub use chat::ChatHandler;
pub use model::AiModel;
pub use model_manager::{
    catalog_model, version_is_newer, InstalledModel, ModelDescriptor, ModelManager, ModelManifest,
    ModelState, ModelStatusInfo, DEFAULT_MODEL_ID, MODEL_CATALOG,
};
