//! Picks which providers run from the ones compiled in, by id.

use std::collections::HashSet;
use std::sync::Arc;

use crate::{ApiAdapter, Notifier, PostProcessor, ProviderInfo};

/// One provider, before it is enabled.
#[derive(Clone)]
pub enum Provider {
    PostProcessor(Arc<dyn PostProcessor>),
    Notifier(Arc<dyn Notifier>),
    ApiAdapter(Arc<dyn ApiAdapter>),
}

impl Provider {
    #[must_use]
    pub fn info(&self) -> ProviderInfo {
        match self {
            Self::PostProcessor(p) => p.info(),
            Self::Notifier(n) => n.info(),
            Self::ApiAdapter(a) => a.info(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// Config names a provider that isn't compiled in.
    Unknown { id: String, available: Vec<String> },
    /// Two compiled-in providers claim the same id.
    DuplicateId(String),
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unknown { id, available } if available.is_empty() => {
                write!(
                    f,
                    "unknown provider `{id}`; this build has no optional providers"
                )
            }
            Self::Unknown { id, available } => {
                write!(
                    f,
                    "unknown provider `{id}`; available: {}",
                    available.join(", ")
                )
            }
            Self::DuplicateId(id) => write!(f, "two providers use the id `{id}`"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// The enabled providers, in the order the available list gave them; post-processors run in it.
#[derive(Clone, Default)]
pub struct Registry {
    pub post_processors: Vec<Arc<dyn PostProcessor>>,
    pub notifiers: Vec<Arc<dyn Notifier>>,
    pub adapters: Vec<Arc<dyn ApiAdapter>>,
    listing: Vec<(ProviderInfo, bool)>,
}

impl Registry {
    /// Enables the providers whose ids appear in `enabled`; everything else stays off.
    ///
    /// # Errors
    /// When `enabled` names an id that isn't in `available`, or two available providers share an id.
    pub fn build(available: Vec<Provider>, enabled: &[String]) -> Result<Self, RegistryError> {
        let mut seen = HashSet::new();
        for provider in &available {
            let id = provider.info().id;
            if !seen.insert(id.clone()) {
                return Err(RegistryError::DuplicateId(id));
            }
        }
        if let Some(unknown) = enabled.iter().find(|id| !seen.contains(id.as_str())) {
            let mut ids: Vec<String> = seen.into_iter().collect();
            ids.sort();
            return Err(RegistryError::Unknown {
                id: unknown.clone(),
                available: ids,
            });
        }

        let mut registry = Self::default();
        for provider in available {
            let info = provider.info();
            let on = enabled.contains(&info.id);
            registry.listing.push((info, on));
            if !on {
                continue;
            }
            match provider {
                Provider::PostProcessor(p) => registry.post_processors.push(p),
                Provider::Notifier(n) => registry.notifiers.push(n),
                Provider::ApiAdapter(a) => registry.adapters.push(a),
            }
        }
        Ok(registry)
    }

    /// Every compiled-in provider and whether it's enabled, for settings pages and startup logs.
    #[must_use]
    pub fn listing(&self) -> &[(ProviderInfo, bool)] {
        &self.listing
    }

    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.post_processors.len() + self.notifiers.len() + self.adapters.len()
    }
}
