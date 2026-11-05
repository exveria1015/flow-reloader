//! Code leases independent of the optional native loader.
use std::sync::Arc;

pub(crate) struct GenerationOwner {
    pub(crate) code: Arc<dyn Send + Sync>,
    #[cfg(feature = "native")]
    pub(crate) id: crate::ArtifactId,
}
impl GenerationOwner {
    pub(crate) fn prepared(&self) {
        #[cfg(feature = "native")]
        crate::retention::update(self.id, |v| {
            v.state = crate::ImageState::Prepared;
            v.outcome = crate::ImageOutcome::Candidate;
        });
    }
    pub(crate) fn activate(&self) {
        #[cfg(feature = "native")]
        crate::retention::update(self.id, |v| {
            v.state = crate::ImageState::Active;
            v.outcome = crate::ImageOutcome::Adopted;
        });
    }
    pub(crate) fn retire(&self) {
        #[cfg(feature = "native")]
        crate::retention::update(self.id, |v| v.state = crate::ImageState::Retired);
    }

    pub(crate) fn lease(&self) -> Arc<dyn Send + Sync> {
        self.code.clone()
    }
}
impl Drop for GenerationOwner {
    fn drop(&mut self) {
        #[cfg(feature = "native")]
        crate::retention::update(self.id, |v| {
            v.state = crate::ImageState::Retired;
            v.outcome = match v.outcome {
                crate::ImageOutcome::Pending => crate::ImageOutcome::Rejected,
                crate::ImageOutcome::Candidate => crate::ImageOutcome::Abandoned,
                other => other,
            };
        });
    }
}
